use crate::db::Shared;
use crate::perm::LoginUser;
use crate::resp::{self, Rv};
use crate::util::now_str;
use axum::extract::{Multipart, State};
use axum::http::HeaderMap;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{json, Value};

fn profile_user(db: &rusqlite::Connection, user_id: i64) -> Value {
    let mut u = crate::util::query_one(
        db,
        "SELECT u.user_id, u.tenant_id, u.dept_id, u.user_name, u.nick_name, u.user_type, u.email, u.phonenumber, u.sex, u.avatar, u.status, u.del_flag, u.login_ip, u.login_date, u.create_dept, u.create_by, u.create_time, u.update_by, u.update_time, u.remark, d.dept_name AS deptName, o.url AS avatarUrl
         FROM sys_user u LEFT JOIN sys_dept d ON d.dept_id = u.dept_id LEFT JOIN sys_oss o ON o.oss_id = u.avatar
         WHERE u.user_id = ?",
        [user_id],
    );
    if let Some(obj) = u.as_object_mut() {
        obj.remove("password");
    }
    u
}

fn role_group(db: &rusqlite::Connection, user_id: i64) -> String {
    let mut names = Vec::new();
    if user_id == 1 {
        return "超级管理员".to_string();
    }
    if let Ok(mut stmt) = db.prepare(
        "SELECT r.role_name FROM sys_user_role ur JOIN sys_role r ON r.role_id = ur.role_id WHERE ur.user_id = ? AND r.del_flag = '0'",
    ) {
        if let Ok(rows) = stmt.query_map([user_id], |r| r.get::<_, String>(0)) {
            names.extend(rows.flatten());
        }
    }
    names.join(",")
}

fn post_group(db: &rusqlite::Connection, user_id: i64) -> String {
    let mut names = Vec::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT p.post_name FROM sys_user_post up JOIN sys_post p ON p.post_id = up.post_id WHERE up.user_id = ?",
    ) {
        if let Ok(rows) = stmt.query_map([user_id], |r| r.get::<_, String>(0)) {
            names.extend(rows.flatten());
        }
    }
    names.join(",")
}

/// GET /system/user/profile
pub async fn get(State(st): State<Shared>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    resp::ok(json!({
        "user": profile_user(&db, user.user_id),
        "roleGroup": role_group(&db, user.user_id),
        "postGroup": post_group(&db, user.user_id),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileForm {
    pub nick_name: String,
    pub phonenumber: String,
    pub email: String,
    pub sex: String,
}

impl Default for ProfileForm {
    fn default() -> Self {
        ProfileForm {
            nick_name: String::new(),
            phonenumber: String::new(),
            email: String::new(),
            sex: "2".into(),
        }
    }
}

/// PUT /system/user/profile
pub async fn update(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: ProfileForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if f.nick_name.is_empty() {
        return resp::fail("用户昵称不能为空");
    }
    if f.phonenumber.len() > 11 {
        return resp::fail("手机号码长度不能超过11个字符");
    }
    let db = st.db.lock().await;
    // unique phone check
    let dup: Option<i64> = db
        .query_row(
            "SELECT user_id FROM sys_user WHERE phonenumber = ? AND phonenumber != '' AND user_id != ? AND del_flag = '0'",
            rusqlite::params![&f.phonenumber, user.user_id],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    if dup.is_some() {
        return resp::fail("修改用户失败，手机号码已存在");
    }
    let _ = db.execute(
        "UPDATE sys_user SET nick_name = ?, phonenumber = ?, email = ?, sex = ?, update_by = ?, update_time = ? WHERE user_id = ?",
        rusqlite::params![f.nick_name, f.phonenumber, f.email, f.sex, user.user_id, now_str(), user.user_id],
    );
    resp::ok_msg(json!({}), "修改成功")
}

/// PUT /system/user/profile/updatePwd (frontend sends isEncrypt:true)
pub async fn update_pwd(State(st): State<Shared>, user: LoginUser, headers: HeaderMap, body: String) -> Rv {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct P {
        old_password: String,
        new_password: String,
    }
    let p: P = match serde_json::from_str(&crate::api::maybe_decrypt(&st, &headers, body).await) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if p.new_password.len() < 5 || p.new_password.len() > 30 {
        return resp::fail("密码长度必须在5到30个字符之间");
    }
    let db = st.db.lock().await;
    let hash: String = db
        .query_row("SELECT password FROM sys_user WHERE user_id = ?", [user.user_id], |r| r.get(0))
        .unwrap_or_default();
    if !bcrypt::verify(&p.old_password, &hash).unwrap_or(false) {
        return resp::fail("修改密码失败，旧密码错误");
    }
    let new_hash = match bcrypt::hash(&p.new_password, 10) {
        Ok(h) => h,
        Err(_) => return resp::fail("密码加密失败"),
    };
    let _ = db.execute(
        "UPDATE sys_user SET password = ?, update_by = ?, update_time = ? WHERE user_id = ?",
        rusqlite::params![new_hash, user.user_id, now_str(), user.user_id],
    );
    resp::ok_msg(json!({}), "修改密码成功")
}

/// POST /system/user/profile/avatar — multipart file upload, stored on disk,
/// recorded in sys_oss; the returned ossId/url pair feeds the avatar field.
pub async fn upload_avatar(State(st): State<Shared>, user: LoginUser, mut multipart: Multipart) -> Rv {
    let db = st.db.lock().await;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if field.file_name().is_none() {
            continue;
        }
        let filename = field.file_name().unwrap_or("file").to_string();
        let data = match field.bytes().await {
            Ok(b) => b,
            Err(_) => return resp::fail("读取上传文件失败"),
        };
        if data.is_empty() {
            continue;
        }
        let suffix: String = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(10)
            .collect::<String>()
            .to_lowercase();
        let stored = format!("{}.{}", crate::auth::rand_hex(16), suffix);
        let dir = std::path::Path::new(&st.cfg.upload_dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            return resp::fail(&format!("创建上传目录失败: {e}"));
        }
        if let Err(e) = std::fs::write(dir.join(&stored), &data) {
            return resp::fail(&format!("保存文件失败: {e}"));
        }
        let oss_id = st.ids.next();
        let url = format!("/uploads/{stored}");
        let original = filename.trim_end_matches(&format!(".{suffix}")).to_string();
        if db
            .execute(
                "INSERT INTO sys_oss (oss_id, tenant_id, file_name, original_name, file_suffix, url, service, create_dept, create_by, create_time)
                 VALUES (?, '000000', ?, ?, ?, ?, 'local', ?, ?, ?)",
                rusqlite::params![oss_id, stored, original, suffix, url, user.dept_id, user.user_id, now_str()],
            )
            .is_err()
        {
            return resp::fail("记录文件信息失败");
        }
        let _ = name;
        return resp::ok(json!({
            "ossId": oss_id,
            "url": url,
            "fileName": stored,
            "originalName": original,
            "fileSuffix": suffix,
        }));
    }
    resp::fail("未接收到文件")
}
