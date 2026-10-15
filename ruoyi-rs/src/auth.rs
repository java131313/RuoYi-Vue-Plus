use crate::captcha;
use crate::db::{get_config, Shared};
use crate::perm;
use crate::resp::{self, Rv};
use crate::{crypto, jwt};
use crate::session::{self, Session};
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LoginBody {
    pub client_id: String,
    pub grant_type: String,
    pub tenant_id: String,
    pub code: String,
    pub uuid: String,
    pub username: String,
    pub password: String,
}

impl Default for LoginBody {
    fn default() -> Self {
        LoginBody {
            client_id: String::new(),
            grant_type: "password".into(),
            tenant_id: "000000".into(),
            code: String::new(),
            uuid: String::new(),
            username: String::new(),
            password: String::new(),
        }
    }
}

pub(crate) fn rand_hex(n: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..n).map(|_| format!("{:x}", rng.gen_range(0..16))).collect()
}

/// POST /auth/login — body may be RSA+AES encrypted (encrypt-key header),
/// which is what plus-ui always sends.
#[axum::debug_handler]
pub async fn login(State(st): State<Shared>, headers: HeaderMap, body: String) -> Rv {
    let raw = if let Some(enc) = headers.get("encrypt-key").and_then(|v| v.to_str().ok()) {
        let key = match &st.rsa_key {
            Some(k) => k,
            None => return resp::fail("服务端未配置解密密钥"),
        };
        match crypto::decrypt_body(key, enc, &body) {
            Ok(p) => p,
            Err(e) => return resp::fail(&format!("请求解密失败: {e}")),
        }
    } else {
        body
    };
    let lb: LoginBody = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if lb.username.is_empty() || lb.password.is_empty() {
        return resp::fail("用户名或密码不能为空");
    }

    let db = st.db.lock().await;

    // 1. client check (sys_client)
    let client = crate::util::query_one(
        &db,
        "SELECT * FROM sys_client WHERE client_id = ? AND del_flag = '0'",
        [&lb.client_id],
    );
    if client.is_null() || client["grantType"].as_str().unwrap_or("").is_empty() {
        return resp::fail("授权类型或客户端id错误");
    }
    let grants = client["grantType"].as_str().unwrap_or("");
    if !grants.split(',').any(|g| g.trim() == lb.grant_type) {
        return resp::fail("授权类型或客户端id错误");
    }
    if client["status"].as_str().unwrap_or("0") != "0" {
        return resp::fail("客户端已被禁用");
    }
    let client_timeout: u64 = client["timeout"].as_i64().unwrap_or(604800).max(1) as u64;

    // 2. tenant check (single-tenant: only '000000' exists)
    let tenant_id = if lb.tenant_id.is_empty() { "000000" } else { &lb.tenant_id };
    let tenant_ok = crate::util::query_one(
        &db,
        "SELECT 1 AS ok FROM sys_tenant WHERE tenant_id = ? AND status = '0'",
        [tenant_id],
    );
    if tenant_ok.is_null() {
        return resp::fail("租户不存在或已停用");
    }

    if lb.grant_type != "password" {
        return resp::fail("暂不支持该认证类型");
    }

    // 3. captcha (read-and-delete, like Redis GETDEL)
    if st.cfg.captcha_enabled {
        let expected = {
            let mut caps = st.captchas.lock().await;
            caps.remove(&lb.uuid).filter(|c| c.expires > Instant::now()).map(|c| c.answer)
        };
        match expected {
            Some(ans) if ans.eq_ignore_ascii_case(&lb.code) => {}
            _ => return resp::fail("验证码错误或已失效"),
        }
    }

    // 4. password retry lockout
    {
        let mut fails = st.pwd_fails.lock().await;
        let now = Instant::now();
        let locked = match fails.get_mut(&lb.username) {
            Some(f) => {
                if let Some(t) = f.locked_until {
                    if now >= t {
                        f.count = 0;
                        f.locked_until = None;
                    }
                }
                f.is_locked()
            }
            None => false,
        };
        if locked {
            return resp::warn(&format!(
                "密码输入错误{}次，帐户锁定{}分钟",
                st.cfg.pwd_max_retry, st.cfg.pwd_lock_minutes
            ));
        }
    }

    // 5. user lookup + bcrypt
    let user = crate::util::query_one(
        &db,
        "SELECT u.*, d.dept_name AS deptName FROM sys_user u LEFT JOIN sys_dept d ON d.dept_id = u.dept_id WHERE u.user_name = ? AND u.del_flag = '0'",
        [&lb.username],
    );
    if user.is_null() {
        return resp::fail("用户不存在或密码错误");
    }
    let user_id = user["userId"].as_i64().unwrap_or(0);
    let hash = user["password"].as_str().unwrap_or("");
    let ok = bcrypt::verify(&lb.password, hash).unwrap_or(false);
    if !ok {
        record_logininfor(&st, &db, &lb.username, client["clientKey"].as_str().unwrap_or(""), client["deviceType"].as_str().unwrap_or(""), "1", "密码输入错误", &headers);
        let mut fails = st.pwd_fails.lock().await;
        let f = fails.entry(lb.username.clone()).or_insert(session::PwdFail { count: 0, locked_until: None });
        f.count += 1;
        if f.count >= st.cfg.pwd_max_retry {
            f.locked_until = Some(Instant::now() + Duration::from_secs(st.cfg.pwd_lock_minutes * 60));
        }
        return resp::fail("用户不存在或密码错误");
    }
    fails_reset(&st, &lb.username).await;
    if user["status"].as_str().unwrap_or("0") != "0" {
        return resp::fail("账号已被停用");
    }

    // 6. issue token + session
    let token_id = rand_hex(32);
    let (ip, browser, os) = client_meta(&headers);
    let sess = Session {
        token_id: token_id.clone(),
        user_id,
        user_name: lb.username.clone(),
        dept_id: user["deptId"].as_i64().unwrap_or(0),
        dept_name: user["deptName"].as_str().unwrap_or("").to_string(),
        client_id: lb.client_id.clone(),
        ip: ip.clone(),
        browser: browser.clone(),
        os: os.clone(),
        login_time: crate::util::now_str(),
    };
    st.sessions.lock().await.insert(token_id.clone(), sess);
    let _ = db.execute(
        "UPDATE sys_user SET login_ip = ?, login_date = ? WHERE user_id = ?",
        rusqlite::params![ip, crate::util::now_str(), user_id],
    );
    record_logininfor(&st, &db, &lb.username, client["clientKey"].as_str().unwrap_or(""), client["deviceType"].as_str().unwrap_or(""), "0", "登录成功", &headers);

    let access_token = jwt::sign(&st.cfg.jwt_secret, &token_id, user_id, &lb.username, &lb.client_id, client_timeout);
    resp::ok(json!({
        "access_token": access_token,
        "refresh_token": "",
        "expire_in": client_timeout,
        "refresh_expire_in": client_timeout,
        "client_id": lb.client_id,
        "scope": "",
        "openid": ""
    }))
}

async fn fails_reset(st: &Shared, username: &str) {
    st.pwd_fails.lock().await.remove(username);
}

fn record_logininfor(
    st: &Shared,
    db: &rusqlite::Connection,
    username: &str,
    client_key: &str,
    device_type: &str,
    status: &str,
    msg: &str,
    headers: &HeaderMap,
) {
    let (ip, browser, os) = client_meta(headers);
    let _ = db.execute(
        "INSERT INTO sys_logininfor (info_id, tenant_id, user_name, client_key, device_type, ipaddr, login_location, browser, os, status, msg, login_time)
         VALUES (?, '000000', ?, ?, ?, ?, '', ?, ?, ?, ?, ?)",
        rusqlite::params![st.ids.next(), username, client_key, device_type, ip, browser, os, status, msg, crate::util::now_str()],
    );
}

fn client_meta(headers: &HeaderMap) -> (String, String, String) {
    let ip = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next().map(|s| s.trim().to_string()))
        .unwrap_or_default();
    let ua = headers.get("User-Agent").and_then(|v| v.to_str().ok()).unwrap_or("");
    let (b, o) = perm::browser_os(ua);
    (ip, b, o)
}

/// POST /auth/logout
pub async fn logout(State(st): State<Shared>, user: perm::LoginUser) -> Rv {
    st.sessions.lock().await.remove(&user.token_id);
    resp::ok_msg(json!({}), "退出成功")
}

/// GET /auth/code
pub async fn code(State(st): State<Shared>) -> Rv {
    if !st.cfg.captcha_enabled {
        return resp::ok(json!({ "captchaEnabled": false }));
    }
    let (text, answer) = captcha::gen_math();
    let uuid = rand_hex(32);
    st.captchas.lock().await.insert(
        uuid.clone(),
        session::Captcha { answer, expires: Instant::now() + Duration::from_secs(120) },
    );
    resp::ok(json!({
        "captchaEnabled": true,
        "uuid": uuid,
        "img": captcha::render_png_base64(&text)
    }))
}

/// GET /auth/tenant/list
pub async fn tenant_list(State(st): State<Shared>) -> Rv {
    let db = st.db.lock().await;
    let rows = crate::util::query_json(
        &db,
        "SELECT tenant_id AS tenantId, company_name AS companyName, domain FROM sys_tenant WHERE status = '0' ORDER BY id",
        rusqlite::params![],
    );
    resp::ok(json!({
        "tenantEnabled": true,
        "voList": rows
    }))
}

/// POST /auth/register — same encryption as login; gated by sys_config
/// `sys.account.registerUser`.
pub async fn register(State(st): State<Shared>, headers: HeaderMap, body: String) -> Rv {
    let raw = if let Some(enc) = headers.get("encrypt-key").and_then(|v| v.to_str().ok()) {
        let key = match &st.rsa_key {
            Some(k) => k,
            None => return resp::fail("服务端未配置解密密钥"),
        };
        match crypto::decrypt_body(key, enc, &body) {
            Ok(p) => p,
            Err(e) => return resp::fail(&format!("请求解密失败: {e}")),
        }
    } else {
        body
    };
    let v: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let db = st.db.lock().await;
    if get_config(&db, "sys.account.registerUser").as_deref() != Some("true") {
        return resp::fail("当前系统没有开启注册功能！");
    }
    let username = v["username"].as_str().unwrap_or("").trim().to_string();
    let password = v["password"].as_str().unwrap_or("").to_string();
    if username.len() < 2 || username.len() > 30 {
        return resp::fail("用户名长度必须在2到30个字符之间");
    }
    if password.len() < 5 || password.len() > 30 {
        return resp::fail("密码长度必须在5到30个字符之间");
    }
    let exists = crate::util::query_one(&db, "SELECT 1 AS ok FROM sys_user WHERE user_name = ?", [&username]);
    if !exists.is_null() {
        return resp::fail("注册失败，该账号已注册");
    }
    let hash = match bcrypt::hash(&password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => return resp::fail("注册失败"),
    };
    let uid = st.ids.next();
    let _ = db.execute(
        "INSERT INTO sys_user (user_id, tenant_id, dept_id, user_name, nick_name, user_type, email, phonenumber, sex, avatar, password, status, del_flag, create_dept, create_by, create_time, remark)
         VALUES (?, '000000', NULL, ?, ?, 'sys_user', '', '', '2', NULL, ?, '0', '0', NULL, NULL, ?, '')",
        rusqlite::params![uid, username, username, hash, crate::util::now_str()],
    );
    resp::ok_msg(json!({}), "注册成功")
}



