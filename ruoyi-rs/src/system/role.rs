use crate::db::Shared;
use crate::pager::PageQuery;
use crate::perm::{self, LoginUser};
use crate::resp::{self, Rv};
use crate::util::{like_wrap, now_str, query_json, query_one};
use axum::extract::{Path, Query, State};
use rusqlite::ToSql;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoleQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub role_name: String,
    pub role_key: String,
    pub status: String,
    pub role_ids: String,
    pub begin_time: String,
    pub end_time: String,
}

impl Default for RoleQuery {
    fn default() -> Self {
        RoleQuery {
            page: PageQuery::default(),
            role_name: String::new(),
            role_key: String::new(),
            status: String::new(),
            role_ids: String::new(),
            begin_time: String::new(),
            end_time: String::new(),
        }
    }
}

const ROLE_COLS: &str =
    "role_id, role_name, role_key, role_sort, data_scope, menu_check_strictly, dept_check_strictly, status, del_flag, create_dept, create_by, create_time, update_by, update_time, remark";

fn role_rows(db: &rusqlite::Connection, cond: &str, params: &[&dyn ToSql], order_limit: &str) -> Vec<Value> {
    query_json(
        db,
        &format!("SELECT {ROLE_COLS} FROM sys_role r WHERE {cond}{order_limit}"),
        params,
    )
}

/// GET /system/role/list
pub async fn list(State(st): State<Shared>, Query(q): Query<RoleQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:list") {
        return denied.into();
    }
    let mut cond = "r.del_flag = '0'".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.role_name.is_empty() {
        cond.push_str(" AND r.role_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.role_name));
    }
    if !q.role_key.is_empty() {
        cond.push_str(" AND r.role_key LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.role_key));
    }
    if !q.status.is_empty() {
        cond.push_str(" AND r.status = ?");
        params.push(q.status.clone());
    }
    if !q.begin_time.is_empty() {
        cond.push_str(" AND substr(r.create_time,1,10) >= substr(?,1,10)");
        params.push(q.begin_time.clone());
    }
    if !q.end_time.is_empty() {
        cond.push_str(" AND substr(r.create_time,1,10) <= substr(?,1,10)");
        params.push(q.end_time.clone());
    }
    let order = q.page.order_by(&["role_id", "role_sort", "role_name", "role_key", "create_time"]);
    let order = if order.is_empty() { " ORDER BY r.role_sort".to_string() } else { order };
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let total: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM sys_role r WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = role_rows(&db, &cond, refs.as_slice(), &format!("{order}{}", q.page.limit_sql()));
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// GET /system/role/optionselect?roleIds=
pub async fn optionselect(State(st): State<Shared>, Query(q): Query<RoleQuery>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:list") {
        return denied;
    }
    let mut cond = "r.del_flag = '0'".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.role_ids.is_empty() {
        let ids: Vec<&str> = q.role_ids.split(',').filter(|s| !s.is_empty()).collect();
        if !ids.is_empty() {
            cond.push_str(&format!(" AND r.role_id IN ({})", ids.iter().map(|_| "?").collect::<Vec<_>>().join(",")));
            params.extend(ids.iter().map(|s| s.to_string()));
        }
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let rows = role_rows(&db, &cond, refs.as_slice(), " ORDER BY r.role_sort");
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/role/{roleId}
pub async fn get_by_id(State(st): State<Shared>, Path(role_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:query") {
        return denied;
    }
    let mut r = query_one(&db, &format!("SELECT {ROLE_COLS} FROM sys_role r WHERE r.role_id = ? AND r.del_flag = '0'"), [role_id]);
    if r.is_null() {
        return resp::fail("角色不存在");
    }
    let menu_ids: Vec<i64> = {
        let mut out = Vec::new();
        if let Ok(mut stmt) = db.prepare("SELECT menu_id FROM sys_role_menu WHERE role_id = ?") {
            if let Ok(rows) = stmt.query_map([role_id], |row| row.get::<_, i64>(0)) {
                out.extend(rows.flatten());
            }
        }
        out
    };
    if let Some(obj) = r.as_object_mut() {
        obj.insert("menuIds".into(), json!(menu_ids));
        obj.insert("admin".into(), json!(role_id == 1));
    }
    resp::ok(r)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoleForm {
    pub role_id: Option<i64>,
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i64,
    pub data_scope: String,
    pub menu_check_strictly: bool,
    pub dept_check_strictly: bool,
    pub status: String,
    pub remark: String,
    pub menu_ids: Vec<i64>,
    pub dept_ids: Vec<i64>,
}

impl Default for RoleForm {
    fn default() -> Self {
        RoleForm {
            role_id: None,
            role_name: String::new(),
            role_key: String::new(),
            role_sort: 0,
            data_scope: "1".into(),
            menu_check_strictly: true,
            dept_check_strictly: true,
            status: "0".into(),
            remark: String::new(),
            menu_ids: Vec::new(),
            dept_ids: Vec::new(),
        }
    }
}

fn valid(f: &RoleForm) -> Option<&'static str> {
    if f.role_name.is_empty() {
        return Some("角色名称不能为空");
    }
    if f.role_key.is_empty() {
        return Some("权限字符不能为空");
    }
    if f.role_sort < 0 {
        return Some("显示顺序不能为负");
    }
    None
}

fn sync_role_menus(db: &rusqlite::Connection, role_id: i64, menu_ids: &[i64]) {
    let _ = db.execute("DELETE FROM sys_role_menu WHERE role_id = ?", [role_id]);
    for m in menu_ids {
        let _ = db.execute("INSERT OR IGNORE INTO sys_role_menu (role_id, menu_id) VALUES (?, ?)", rusqlite::params![role_id, m]);
    }
}

/// POST /system/role
pub async fn add(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: RoleForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if let Some(msg) = valid(&f) {
        return resp::fail(msg);
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:add") {
        return denied;
    }
    let dup = query_one(&db, "SELECT 1 AS ok FROM sys_role WHERE role_name = ? AND del_flag = '0'", [&f.role_name]);
    if !dup.is_null() {
        return resp::fail("新增角色失败，角色名称已存在");
    }
    let dup = query_one(&db, "SELECT 1 AS ok FROM sys_role WHERE role_key = ? AND del_flag = '0'", [&f.role_key]);
    if !dup.is_null() {
        return resp::fail("新增角色失败，权限字符已存在");
    }
    let rid = st.ids.next();
    let mc: i64 = f.menu_check_strictly.into();
    let dc: i64 = f.dept_check_strictly.into();
    if db
        .execute(
            "INSERT INTO sys_role (role_id, tenant_id, role_name, role_key, role_sort, data_scope, menu_check_strictly, dept_check_strictly, status, del_flag, create_dept, create_by, create_time, remark)
             VALUES (?, '000000', ?, ?, ?, ?, ?, ?, ?, '0', ?, ?, ?, ?)",
            rusqlite::params![rid, f.role_name, f.role_key, f.role_sort, f.data_scope, mc, dc, f.status, user.dept_id, user.user_id, now_str(), f.remark],
        )
        .is_err()
    {
        return resp::fail("新增角色失败");
    }
    sync_role_menus(&db, rid, &f.menu_ids);
    crate::monitor::log_oper(&st, &db, &user, "角色管理", 1, "POST", "/system/role", &f.role_name, 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/role
pub async fn edit(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: RoleForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(rid) = f.role_id else { return resp::fail("角色ID不能为空") };
    if rid == 1 {
        return resp::fail("不允许操作超级管理员角色");
    }
    if let Some(msg) = valid(&f) {
        return resp::fail(msg);
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:edit") {
        return denied;
    }
    let dup = query_one(
        &db,
        "SELECT 1 AS ok FROM sys_role WHERE role_name = ? AND role_id != ? AND del_flag = '0'",
        rusqlite::params![&f.role_name, rid],
    );
    if !dup.is_null() {
        return resp::fail("修改角色失败，角色名称已存在");
    }
    let mc: i64 = f.menu_check_strictly.into();
    let dc: i64 = f.dept_check_strictly.into();
    if db
        .execute(
            "UPDATE sys_role SET role_name = ?, role_key = ?, role_sort = ?, data_scope = ?, menu_check_strictly = ?, dept_check_strictly = ?, status = ?, remark = ?, update_by = ?, update_time = ? WHERE role_id = ?",
            rusqlite::params![f.role_name, f.role_key, f.role_sort, f.data_scope, mc, dc, f.status, f.remark, user.user_id, now_str(), rid],
        )
        .is_err()
    {
        return resp::fail("修改角色失败");
    }
    sync_role_menus(&db, rid, &f.menu_ids);
    crate::monitor::log_oper(&st, &db, &user, "角色管理", 2, "PUT", "/system/role", &f.role_name, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// PUT /system/role/dataScope
pub async fn data_scope(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: RoleForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(rid) = f.role_id else { return resp::fail("角色ID不能为空") };
    if rid == 1 {
        return resp::fail("不允许操作超级管理员角色");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:edit") {
        return denied;
    }
    let _ = db.execute(
        "UPDATE sys_role SET data_scope = ?, update_by = ?, update_time = ? WHERE role_id = ?",
        rusqlite::params![f.data_scope, user.user_id, now_str(), rid],
    );
    let _ = db.execute("DELETE FROM sys_role_dept WHERE role_id = ?", [rid]);
    for d in &f.dept_ids {
        let _ = db.execute("INSERT OR IGNORE INTO sys_role_dept (role_id, dept_id) VALUES (?, ?)", rusqlite::params![rid, d]);
    }
    crate::monitor::log_oper(&st, &db, &user, "角色管理", 4, "PUT", "/system/role/dataScope", &format!("roleId:{rid}"), 0, "", 0);
    resp::ok_msg(json!({}), "数据权限配置成功")
}

/// PUT /system/role/changeStatus
pub async fn change_status(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct P {
        role_id: i64,
        status: String,
    }
    let p: P = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if p.role_id == 1 {
        return resp::fail("不允许操作超级管理员角色");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:edit") {
        return denied;
    }
    let _ = db.execute(
        "UPDATE sys_role SET status = ?, update_by = ?, update_time = ? WHERE role_id = ?",
        rusqlite::params![p.status, user.user_id, now_str(), p.role_id],
    );
    crate::monitor::log_oper(&st, &db, &user, "角色管理", 2, "PUT", "/system/role/changeStatus", &format!("roleId:{}", p.role_id), 0, "", 0);
    resp::ok_msg(json!({}), "操作成功")
}

/// DELETE /system/role/{roleIds}
pub async fn remove(State(st): State<Shared>, Path(role_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:remove") {
        return denied;
    }
    for id in role_ids.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        if id == 1 {
            return resp::fail("不允许删除超级管理员角色");
        }
        let used = query_one(&db, "SELECT 1 AS ok FROM sys_user_role WHERE role_id = ? LIMIT 1", [id]);
        if !used.is_null() {
            return resp::fail("角色已分配用户，不能删除");
        }
        let _ = db.execute("UPDATE sys_role SET del_flag = '2', update_by = ?, update_time = ? WHERE role_id = ?", rusqlite::params![user.user_id, now_str(), id]);
    }
    crate::monitor::log_oper(&st, &db, &user, "角色管理", 3, "DELETE", "/system/role", &role_ids, 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}

fn auth_user_list(db: &rusqlite::Connection, role_id: i64, user_name: &str, allocated: bool, page: &PageQuery) -> crate::resp::TableDataInfo {
    let mut cond = String::from("u.del_flag = '0'");
    let mut params: Vec<String> = Vec::new();
    if allocated {
        cond.push_str(" AND u.user_id IN (SELECT user_id FROM sys_user_role WHERE role_id = ?)");
        params.push(role_id.to_string());
    } else {
        cond.push_str(" AND u.user_id NOT IN (SELECT user_id FROM sys_user_role WHERE role_id = ?)");
        params.push(role_id.to_string());
    }
    if !user_name.is_empty() {
        cond.push_str(" AND u.user_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(user_name));
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let total: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM sys_user u WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        db,
        &format!(
            "SELECT u.user_id, u.tenant_id, u.dept_id, u.user_name, u.nick_name, u.user_type, u.email, u.phonenumber, u.sex, u.avatar, u.status, u.del_flag, u.login_ip, u.login_date, u.create_dept, u.create_by, u.create_time, u.update_by, u.update_time, u.remark, d.dept_name AS deptName FROM sys_user u LEFT JOIN sys_dept d ON d.dept_id = u.dept_id WHERE {cond} ORDER BY u.user_id{}",
            page.limit_sql()
        ),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// GET /system/role/authUser/allocatedList?roleId=&userName=&pageNum=&pageSize=
pub async fn allocated_list(State(st): State<Shared>, Query(mut q): Query<std::collections::HashMap<String, String>>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:list") {
        return denied.into();
    }
    let role_id: i64 = q.remove("roleId").and_then(|s| s.parse().ok()).unwrap_or(0);
    let user_name = q.remove("userName").unwrap_or_default();
    let page = PageQuery {
        page_num: q.remove("pageNum").unwrap_or_else(|| "1".into()),
        page_size: q.remove("pageSize").unwrap_or_else(|| "10".into()),
        ..Default::default()
    };
    auth_user_list(&db, role_id, &user_name, true, &page).into()
}

/// GET /system/role/authUser/unallocatedList?roleId=&userName=
pub async fn unallocated_list(State(st): State<Shared>, Query(mut q): Query<std::collections::HashMap<String, String>>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:list") {
        return denied.into();
    }
    let role_id: i64 = q.remove("roleId").and_then(|s| s.parse().ok()).unwrap_or(0);
    let user_name = q.remove("userName").unwrap_or_default();
    let page = PageQuery {
        page_num: q.remove("pageNum").unwrap_or_else(|| "1".into()),
        page_size: q.remove("pageSize").unwrap_or_else(|| "10".into()),
        ..Default::default()
    };
    auth_user_list(&db, role_id, &user_name, false, &page).into()
}

/// PUT /system/role/authUser/cancel {userId, roleId}
pub async fn auth_user_cancel(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct P {
        user_id: i64,
        role_id: i64,
    }
    let p: P = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:edit") {
        return denied;
    }
    let _ = db.execute("DELETE FROM sys_user_role WHERE user_id = ? AND role_id = ?", rusqlite::params![p.user_id, p.role_id]);
    resp::ok_msg(json!({}), "取消授权成功")
}

/// PUT /system/role/authUser/cancelAll?userIds=&roleId=
pub async fn auth_user_cancel_all(State(st): State<Shared>, Query(p): Query<std::collections::HashMap<String, String>>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:edit") {
        return denied;
    }
    let role_id: i64 = p.get("roleId").and_then(|s| s.parse().ok()).unwrap_or(0);
    if let Some(ids) = p.get("userIds") {
        for id in ids.split(',') {
            if let Ok(uid) = id.parse::<i64>() {
                let _ = db.execute("DELETE FROM sys_user_role WHERE user_id = ? AND role_id = ?", rusqlite::params![uid, role_id]);
            }
        }
    }
    resp::ok_msg(json!({}), "取消授权成功")
}

/// PUT /system/role/authUser/selectAll?userIds=&roleId=
pub async fn auth_user_select_all(State(st): State<Shared>, Query(p): Query<std::collections::HashMap<String, String>>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:edit") {
        return denied;
    }
    let role_id: i64 = p.get("roleId").and_then(|s| s.parse().ok()).unwrap_or(0);
    if let Some(ids) = p.get("userIds") {
        for id in ids.split(',') {
            if let Ok(uid) = id.parse::<i64>() {
                let _ = db.execute("INSERT OR IGNORE INTO sys_user_role (user_id, role_id) VALUES (?, ?)", rusqlite::params![uid, role_id]);
            }
        }
    }
    resp::ok_msg(json!({}), "授权成功")
}

/// GET /system/role/deptTree/{roleId} — dept tree + checked keys (sys_role_dept).
pub async fn dept_tree(State(st): State<Shared>, Path(role_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:role:query") {
        return denied;
    }
    let rows = query_json(
        &db,
        "SELECT dept_id, parent_id, dept_name, order_num FROM sys_dept WHERE del_flag = '0' ORDER BY parent_id, order_num",
        rusqlite::params![],
    );
    let checked: Vec<i64> = {
        let mut out = Vec::new();
        if let Ok(mut stmt) = db.prepare("SELECT dept_id FROM sys_role_dept WHERE role_id = ?") {
            if let Ok(rs) = stmt.query_map([role_id], |r| r.get::<_, i64>(0)) {
                out.extend(rs.flatten());
            }
        }
        out
    };
    resp::ok(json!({
        "depts": crate::system::build_tree_by(rows, "dept_id", "dept_name", &[]),
        "checkedKeys": checked,
    }))
}
