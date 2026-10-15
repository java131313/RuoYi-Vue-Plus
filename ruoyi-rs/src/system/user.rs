use crate::db::Shared;
use crate::pager::PageQuery;
use crate::perm::{self, LoginUser};
use crate::resp::{self, Rv};
use crate::system::data_scope_filter;
use crate::util::{like_wrap, now_str, query_json, query_one};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use rusqlite::ToSql;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub user_name: String,
    pub nick_name: String,
    pub phonenumber: String,
    pub status: String,
    pub dept_id: Option<i64>,
    pub role_id: Option<i64>,
    pub user_ids: String,
    pub begin_time: String,
    pub end_time: String,
}

impl Default for UserQuery {
    fn default() -> Self {
        UserQuery {
            page: PageQuery::default(),
            user_name: String::new(),
            nick_name: String::new(),
            phonenumber: String::new(),
            status: String::new(),
            dept_id: None,
            role_id: None,
            user_ids: String::new(),
            begin_time: String::new(),
            end_time: String::new(),
        }
    }
}

const USER_COLS: &str =
    "u.user_id, u.tenant_id, u.dept_id, u.user_name, u.nick_name, u.user_type, u.email, u.phonenumber, u.sex, u.avatar, u.status, u.del_flag, u.login_ip, u.login_date, u.create_dept, u.create_by, u.create_time, u.update_by, u.update_time, u.remark, d.dept_name AS deptName";
const USER_ORDER_COLS: &[&str] = &[
    "user_id", "dept_id", "user_name", "nick_name", "email", "phonenumber", "sex", "status",
    "login_ip", "login_date", "create_time", "update_time",
];

fn select_user(db: &rusqlite::Connection, user_id: i64) -> Value {
    let mut u = query_one(
        db,
        &format!(
            "SELECT {}, o.url AS avatarUrl FROM sys_user u
             LEFT JOIN sys_dept d ON d.dept_id = u.dept_id
             LEFT JOIN sys_oss o ON o.oss_id = u.avatar
             WHERE u.user_id = ? AND u.del_flag = '0'",
            USER_COLS
        ),
        [user_id],
    );
    if let Some(obj) = u.as_object_mut() {
        obj.remove("password");
    }
    u
}

/// GET /system/user/list
pub async fn list(State(st): State<Shared>, Query(q): Query<UserQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:list") {
        return denied.into();
    }
    let mut cond = String::from("u.del_flag = '0'");
    let mut params: Vec<String> = Vec::new();
    if !q.user_name.is_empty() {
        cond.push_str(" AND u.user_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.user_name));
    }
    if !q.nick_name.is_empty() {
        cond.push_str(" AND u.nick_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.nick_name));
    }
    if !q.phonenumber.is_empty() {
        cond.push_str(" AND u.phonenumber LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.phonenumber));
    }
    if !q.status.is_empty() {
        cond.push_str(" AND u.status = ?");
        params.push(q.status.clone());
    }
    if let Some(did) = q.dept_id {
        cond.push_str(" AND u.dept_id = ?");
        params.push(did.to_string());
    }
    if !q.begin_time.is_empty() {
        cond.push_str(" AND substr(u.create_time,1,10) >= substr(?,1,10)");
        params.push(q.begin_time.clone());
    }
    if !q.end_time.is_empty() {
        cond.push_str(" AND substr(u.create_time,1,10) <= substr(?,1,10)");
        params.push(q.end_time.clone());
    }
    if let Some(rid) = q.role_id {
        cond.push_str(" AND u.user_id IN (SELECT ur.user_id FROM sys_user_role ur WHERE ur.role_id = ?)");
        params.push(rid.to_string());
    }
    if !q.user_ids.is_empty() {
        let ids: Vec<&str> = q.user_ids.split(',').filter(|s| !s.is_empty()).collect();
        if !ids.is_empty() {
            cond.push_str(&format!(
                " AND u.user_id IN ({})",
                ids.iter().map(|_| "?").collect::<Vec<_>>().join(",")
            ));
            params.extend(ids.iter().map(|s| s.to_string()));
        }
    }
    if let Some(scope) = data_scope_filter(&db, user.user_id, "u") {
        cond.push_str(&format!(" AND ({scope})"));
    }
    let from_where = format!("FROM sys_user u LEFT JOIN sys_dept d ON d.dept_id = u.dept_id WHERE {cond}");
    let order = q.page.order_by(USER_ORDER_COLS);
    let order = if order.is_empty() { " ORDER BY u.user_id".to_string() } else { order };

    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let total: i64 = db
        .query_row(&format!("SELECT COUNT(*) {from_where}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        &db,
        &format!(
            "SELECT {USER_COLS} {from_where}{} {}",
            order,
            q.page.limit_sql()
        ),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, serde_json::Value::Array(rows)).into()
}

/// GET /system/user/optionselect?userIds=
pub async fn optionselect(State(st): State<Shared>, Query(q): Query<UserQuery>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:list") {
        return denied;
    }
    let mut cond = "u.del_flag = '0'".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.user_ids.is_empty() {
        let ids: Vec<&str> = q.user_ids.split(',').filter(|s| !s.is_empty()).collect();
        if !ids.is_empty() {
            cond.push_str(&format!(
                " AND u.user_id IN ({})",
                ids.iter().map(|_| "?").collect::<Vec<_>>().join(",")
            ));
            params.extend(ids.iter().map(|s| s.to_string()));
        }
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let rows = query_json(
        &db,
        &format!("SELECT {USER_COLS} FROM sys_user u LEFT JOIN sys_dept d ON d.dept_id = u.dept_id WHERE {cond} ORDER BY u.user_id"),
        refs.as_slice(),
    );
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/user/getInfo — current login user + perms + role keys.
pub async fn get_info(State(st): State<Shared>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let u = select_user(&db, user.user_id);
    resp::ok(json!({
        "user": u,
        "permissions": perm::load_perms(&db, user.user_id),
        "roles": perm::load_role_keys(&db, user.user_id),
    }))
}

fn role_list_all(db: &rusqlite::Connection, user_id: i64) -> Vec<Value> {
    // admins see all roles; others see their own roles (Java selectRoleAll)
    let sql = if user_id == 1 {
        "SELECT role_id, role_name, role_key, role_sort, data_scope, menu_check_strictly, dept_check_strictly, status, del_flag, create_dept, create_by, create_time, update_by, update_time, remark FROM sys_role WHERE del_flag = '0' ORDER BY role_sort".to_string()
    } else {
        format!("SELECT role_id, role_name, role_key, role_sort, data_scope, menu_check_strictly, dept_check_strictly, status, del_flag, create_dept, create_by, create_time, update_by, update_time, remark FROM sys_role r WHERE r.del_flag = '0' AND r.role_id IN (SELECT role_id FROM sys_user_role WHERE user_id = {user_id}) ORDER BY role_sort")
    };
    query_json(db, &sql, rusqlite::params![])
}

fn post_list_all(db: &rusqlite::Connection) -> Vec<Value> {
    query_json(
        db,
        "SELECT post_id, tenant_id, dept_id, post_code, post_category, post_name, post_sort, status, create_dept, create_by, create_time, update_by, update_time, remark FROM sys_post ORDER BY post_sort",
        rusqlite::params![],
    )
}

fn assigned_ids(db: &rusqlite::Connection, table: &str, col: &str, user_id: i64) -> Vec<i64> {
    let sql = format!("SELECT {col} FROM {table} WHERE user_id = ?");
    let mut out = Vec::new();
    if let Ok(mut stmt) = db.prepare(&sql) {
        if let Ok(rows) = stmt.query_map([user_id], |r| r.get::<_, i64>(0)) {
            out.extend(rows.flatten());
        }
    }
    out
}

/// GET /system/user (add dialog — empty user + all roles/posts)
pub async fn get_empty(State(st): State<Shared>, user: LoginUser) -> Rv {
    get_full(st, user, 0).await
}

/// GET /system/user/ and GET /system/user/{userId} — detail for add/edit dialog.
pub async fn get_by_id(State(st): State<Shared>, Path(user_id): Path<String>, user: LoginUser) -> Rv {
    let uid: i64 = user_id.parse().unwrap_or(0);
    get_full(st, user, uid).await
}

async fn get_full(st: Shared, user: LoginUser, uid: i64) -> Rv {
    let db = st.db.lock().await;
    if uid > 0 {
        let perms = perm::load_perms(&db, user.user_id);
        if let Some(denied) = perm::check(&user, &perms, "system:user:query") {
            return denied;
        }
    }
    let u = if uid > 0 { select_user(&db, uid) } else { json!({}) };
    let roles = role_list_all(&db, user.user_id);
    let posts = post_list_all(&db);
    let role_ids = assigned_ids(&db, "sys_user_role", "role_id", uid);
    let post_ids = assigned_ids(&db, "sys_user_post", "post_id", uid);
    let role_names: Vec<String> = role_ids
        .iter()
        .filter_map(|id| {
            roles
                .iter()
                .find(|r| r["roleId"].as_i64() == Some(*id))
                .and_then(|r| r["roleName"].as_str().map(|s| s.to_string()))
        })
        .collect();
    let post_names: Vec<String> = post_ids
        .iter()
        .filter_map(|id| {
            posts
                .iter()
                .find(|p| p["postId"].as_i64() == Some(*id))
                .and_then(|p| p["postName"].as_str().map(|s| s.to_string()))
        })
        .collect();
    resp::ok(json!({
        "user": u,
        "roles": roles,
        "roleIds": role_ids,
        "posts": posts,
        "postIds": post_ids,
        "roleGroup": role_names.join(","),
        "postGroup": post_names.join(","),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserForm {
    pub user_id: Option<i64>,
    pub dept_id: Option<i64>,
    pub user_name: String,
    pub nick_name: String,
    pub password: String,
    pub phonenumber: String,
    pub email: String,
    pub sex: String,
    pub status: String,
    pub remark: String,
    pub role_ids: Vec<i64>,
    pub post_ids: Vec<i64>,
}

impl Default for UserForm {
    fn default() -> Self {
        UserForm {
            user_id: None,
            dept_id: None,
            user_name: String::new(),
            nick_name: String::new(),
            password: String::new(),
            phonenumber: String::new(),
            email: String::new(),
            sex: "2".into(),
            status: "0".into(),
            remark: String::new(),
            role_ids: Vec::new(),
            post_ids: Vec::new(),
        }
    }
}

fn valid_user_form(f: &UserForm, creating: bool) -> Option<&'static str> {
    if f.user_name.is_empty() {
        return Some("用户账号不能为空");
    }
    if f.nick_name.is_empty() {
        return Some("用户昵称不能为空");
    }
    if creating && (f.password.len() < 5 || f.password.len() > 30) {
        return Some("密码长度必须在5到30个字符之间");
    }
    if !f.email.is_empty() && !f.email.contains('@') {
        return Some("邮箱格式不正确");
    }
    if !f.phonenumber.is_empty() && f.phonenumber.len() > 11 {
        return Some("手机号码长度不能超过11个字符");
    }
    None
}

fn insert_user_relations(db: &rusqlite::Connection, uid: i64, role_ids: &[i64], post_ids: &[i64]) {
    for rid in role_ids {
        let _ = db.execute("INSERT OR IGNORE INTO sys_user_role (user_id, role_id) VALUES (?, ?)", rusqlite::params![uid, rid]);
    }
    for pid in post_ids {
        let _ = db.execute("INSERT OR IGNORE INTO sys_user_post (user_id, post_id) VALUES (?, ?)", rusqlite::params![uid, pid]);
    }
}

/// POST /system/user
pub async fn add(State(st): State<Shared>, user: LoginUser, headers: HeaderMap, body: String) -> Rv {
    let f: UserForm = match serde_json::from_str(&crate::api::maybe_decrypt(&st, &headers, body).await) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if let Some(msg) = valid_user_form(&f, true) {
        return resp::fail(msg);
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:add") {
        return denied;
    }
    let dup = query_one(&db, "SELECT 1 AS ok FROM sys_user WHERE user_name = ?", [&f.user_name]);
    if !dup.is_null() {
        return resp::fail("新增用户失败，登录账号已存在");
    }
    let hash = match bcrypt::hash(&f.password, 10) {
        Ok(h) => h,
        Err(_) => return resp::fail("密码加密失败"),
    };
    let uid = st.ids.next();
    db.execute(
        "INSERT INTO sys_user (user_id, tenant_id, dept_id, user_name, nick_name, user_type, email, phonenumber, sex, avatar, password, status, del_flag, login_ip, login_date, create_dept, create_by, create_time, remark)
         VALUES (?, '000000', ?, ?, ?, 'sys_user', ?, ?, ?, NULL, ?, ?, '0', '', NULL, ?, ?, ?, ?)",
        rusqlite::params![
            uid,
            f.dept_id,
            f.user_name,
            f.nick_name,
            f.email,
            f.phonenumber,
            f.sex,
            hash,
            f.status,
            user.dept_id,
            user.user_id,
            now_str(),
            f.remark
        ],
    )
    .map_err(|e| e.to_string())
    .unwrap_or(0);
    insert_user_relations(&db, uid, &f.role_ids, &f.post_ids);
    crate::monitor::log_oper(&st, &db, &user, "用户管理", 1, "POST", "/system/user", &format!("用户账号:{}", f.user_name), 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/user
pub async fn edit(State(st): State<Shared>, user: LoginUser, headers: HeaderMap, body: String) -> Rv {
    let f: UserForm = match serde_json::from_str(&crate::api::maybe_decrypt(&st, &headers, body).await) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(uid) = f.user_id else { return resp::fail("用户ID不能为空") };
    if let Some(msg) = valid_user_form(&f, false) {
        return resp::fail(msg);
    }
    if uid == 1 && user.user_id != 1 {
        return resp::fail("不允许操作超级管理员用户");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:edit") {
        return denied;
    }
    let dup = query_one(
        &db,
        "SELECT 1 AS ok FROM sys_user WHERE user_name = ? AND user_id != ? AND del_flag = '0'",
        rusqlite::params![&f.user_name, uid],
    );
    if !dup.is_null() {
        return resp::fail("修改用户失败，登录账号已存在");
    }
    db.execute(
        "UPDATE sys_user SET dept_id = ?, user_name = ?, nick_name = ?, email = ?, phonenumber = ?, sex = ?, status = ?, remark = ?, update_by = ?, update_time = ? WHERE user_id = ?",
        rusqlite::params![
            f.dept_id,
            f.user_name,
            f.nick_name,
            f.email,
            f.phonenumber,
            f.sex,
            f.status,
            f.remark,
            user.user_id,
            now_str(),
            uid
        ],
    )
    .map_err(|e| e.to_string())
    .unwrap_or(0);
    if uid != 1 {
        let _ = db.execute("DELETE FROM sys_user_role WHERE user_id = ?", [uid]);
        let _ = db.execute("DELETE FROM sys_user_post WHERE user_id = ?", [uid]);
        insert_user_relations(&db, uid, &f.role_ids, &f.post_ids);
    }
    crate::monitor::log_oper(&st, &db, &user, "用户管理", 2, "PUT", "/system/user", &format!("用户账号:{}", f.user_name), 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// DELETE /system/user/{userIds}
pub async fn remove(State(st): State<Shared>, Path(user_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:remove") {
        return denied;
    }
    let mut ids: Vec<i64> = user_ids.split(',').filter_map(|s| s.parse().ok()).collect();
    if ids.contains(&1) {
        return resp::fail("不允许删除超级管理员用户");
    }
    ids.retain(|&id| id != user.user_id);
    for id in ids {
        let _ = db.execute("UPDATE sys_user SET del_flag = '2', update_by = ?, update_time = ? WHERE user_id = ?", rusqlite::params![user.user_id, now_str(), id]);
    }
    crate::monitor::log_oper(&st, &db, &user, "用户管理", 3, "DELETE", "/system/user", &user_ids, 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}

/// PUT /system/user/resetPwd (frontend sends isEncrypt:true)
pub async fn reset_pwd(State(st): State<Shared>, user: LoginUser, headers: HeaderMap, body: String) -> Rv {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct P {
        user_id: i64,
        password: String,
    }
    let p: P = match serde_json::from_str(&crate::api::maybe_decrypt(&st, &headers, body).await) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if p.password.len() < 5 || p.password.len() > 30 {
        return resp::fail("密码长度必须在5到30个字符之间");
    }
    if p.user_id == 1 && user.user_id != 1 {
        return resp::fail("不允许操作超级管理员用户");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:resetPwd") {
        return denied;
    }
    let hash = match bcrypt::hash(&p.password, 10) {
        Ok(h) => h,
        Err(_) => return resp::fail("密码加密失败"),
    };
    let _ = db.execute(
        "UPDATE sys_user SET password = ?, update_by = ?, update_time = ? WHERE user_id = ?",
        rusqlite::params![hash, user.user_id, now_str(), p.user_id],
    );
    crate::monitor::log_oper(&st, &db, &user, "用户管理", 2, "PUT", "/system/user/resetPwd", &format!("userId:{}", p.user_id), 0, "", 0);
    resp::ok_msg(json!({}), "重置密码成功")
}

/// PUT /system/user/changeStatus
pub async fn change_status(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct P {
        user_id: i64,
        status: String,
    }
    let p: P = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if p.user_id == 1 && user.user_id != 1 {
        return resp::fail("不允许操作超级管理员用户");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:edit") {
        return denied;
    }
    let _ = db.execute(
        "UPDATE sys_user SET status = ?, update_by = ?, update_time = ? WHERE user_id = ?",
        rusqlite::params![p.status, user.user_id, now_str(), p.user_id],
    );
    crate::monitor::log_oper(&st, &db, &user, "用户管理", 2, "PUT", "/system/user/changeStatus", &format!("userId:{} status:{}", p.user_id, p.status), 0, "", 0);
    resp::ok_msg(json!({}), "操作成功")
}

/// GET /system/user/authRole/{userId}
pub async fn auth_role(State(st): State<Shared>, Path(uid): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:query") {
        return denied;
    }
    let u = select_user(&db, uid);
    let mut roles = role_list_all(&db, user.user_id);
    let assigned: std::collections::HashSet<i64> = assigned_ids(&db, "sys_user_role", "role_id", uid).into_iter().collect();
    for r in roles.iter_mut() {
        let id = r["roleId"].as_i64().unwrap_or(0);
        r.as_object_mut().unwrap().insert("flag".into(), json!(assigned.contains(&id)));
    }
    resp::ok(json!({ "user": u, "roles": roles }))
}

/// PUT /system/user/authRole?userId=&roleIds=
pub async fn save_auth_role(State(st): State<Shared>, Query(p): Query<std::collections::HashMap<String, String>>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:user:edit") {
        return denied;
    }
    let uid: i64 = p.get("userId").and_then(|s| s.parse().ok()).unwrap_or(0);
    if uid == 1 {
        return resp::fail("不允许操作超级管理员用户");
    }
    let _ = db.execute("DELETE FROM sys_user_role WHERE user_id = ?", [uid]);
    if let Some(ids) = p.get("roleIds") {
        for id in ids.split(',') {
            if let Ok(rid) = id.parse::<i64>() {
                let _ = db.execute("INSERT OR IGNORE INTO sys_user_role (user_id, role_id) VALUES (?, ?)", rusqlite::params![uid, rid]);
            }
        }
    }
    crate::monitor::log_oper(&st, &db, &user, "用户管理", 4, "PUT", "/system/user/authRole", &format!("userId:{uid}"), 0, "", 0);
    resp::ok_msg(json!({}), "授权成功")
}

/// GET /system/user/list/dept/{deptId}
pub async fn list_by_dept(State(st): State<Shared>, Path(dept_id): Path<i64>, _user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let rows = query_json(
        &db,
        &format!(
            "SELECT {USER_COLS} FROM sys_user u LEFT JOIN sys_dept d ON d.dept_id = u.dept_id WHERE u.dept_id = ? AND u.del_flag = '0' ORDER BY u.user_id"
        ),
        [dept_id],
    );
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/user/deptTree — used by the user page's dept tree filter.
pub async fn dept_tree(State(st): State<Shared>, _user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let rows = query_json(
        &db,
        "SELECT dept_id, parent_id, dept_name, order_num FROM sys_dept WHERE del_flag = '0' ORDER BY parent_id, order_num",
        rusqlite::params![],
    );
    resp::ok(serde_json::to_value(crate::system::build_tree_by(rows, "dept_id", "dept_name", &[])).unwrap())
}

