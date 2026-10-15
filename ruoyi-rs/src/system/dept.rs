use crate::db::Shared;
use crate::perm::{self, LoginUser};
use crate::resp::{self, Rv};
use crate::util::{like_wrap, now_str, query_json, query_one};
use axum::extract::{Path, Query, State};
use rusqlite::ToSql;
use serde::Deserialize;
use serde_json::{json, Value};

const DEPT_COLS: &str = "d.dept_id, d.tenant_id, d.parent_id, d.ancestors, d.dept_name, d.dept_category, d.order_num, d.leader, d.phone, d.email, d.status, d.del_flag, d.create_dept, d.create_by, d.create_time, d.update_by, d.update_time, u.nick_name AS leaderName";

/// GET /system/dept/list
pub async fn list(State(st): State<Shared>, Query(q): Query<std::collections::HashMap<String, String>>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dept:list") {
        return denied;
    }
    let mut cond = "d.del_flag = '0'".to_string();
    let mut params: Vec<String> = Vec::new();
    if let Some(v) = q.get("deptName").filter(|s| !s.is_empty()) {
        cond.push_str(" AND d.dept_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(v));
    }
    if let Some(v) = q.get("deptCategory").filter(|s| !s.is_empty()) {
        cond.push_str(" AND d.dept_category = ?");
        params.push(v.clone());
    }
    if let Some(v) = q.get("status").filter(|s| !s.is_empty()) {
        cond.push_str(" AND d.status = ?");
        params.push(v.clone());
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let rows = query_json(
        &db,
        &format!("SELECT {DEPT_COLS} FROM sys_dept d LEFT JOIN sys_user u ON u.user_id = d.leader WHERE {cond} ORDER BY d.parent_id, d.order_num"),
        refs.as_slice(),
    );
    // DeptVO exposes both id and deptId
    let rows: Vec<Value> = rows
        .into_iter()
        .map(|mut r| {
            if let Some(obj) = r.as_object_mut() {
                let did = obj.get("deptId").cloned().unwrap_or(Value::Null);
                obj.insert("id".into(), did);
                obj.insert("children".into(), json!([]));
            }
            r
        })
        .collect();
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/dept/optionselect?deptIds= → TreeSelect
pub async fn optionselect(State(st): State<Shared>, Query(q): Query<std::collections::HashMap<String, String>>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dept:query") {
        return denied;
    }
    let mut sql = String::from(
        "SELECT dept_id, parent_id, dept_name, order_num FROM sys_dept WHERE del_flag = '0'",
    );
    if let Some(ids) = q.get("deptIds").filter(|s| !s.is_empty()) {
        let list: Vec<&str> = ids.split(',').filter(|s| !s.is_empty()).collect();
        if !list.is_empty() {
            sql.push_str(&format!(" AND dept_id IN ({})", list.iter().map(|_| "?").collect::<Vec<_>>().join(",")));
        }
    }
    sql.push_str(" ORDER BY parent_id, order_num");
    let rows = query_json(&db, &sql, rusqlite::params![]);
    resp::ok(serde_json::to_value(crate::system::build_tree_by(rows, "dept_id", "dept_name", &[])).unwrap())
}

/// GET /system/dept/list/exclude/{deptId}
pub async fn exclude_child(State(st): State<Shared>, Path(dept_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dept:list") {
        return denied;
    }
    let rows = query_json(
        &db,
        &format!(
            "SELECT {DEPT_COLS} FROM sys_dept d LEFT JOIN sys_user u ON u.user_id = d.leader
             WHERE d.del_flag = '0' AND d.dept_id != ? AND d.ancestors NOT LIKE (SELECT '%' || ancestors || '%' FROM sys_dept WHERE dept_id = ?)
             ORDER BY d.parent_id, d.order_num"
        ),
        rusqlite::params![dept_id, dept_id],
    );
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/dept/{deptId}
pub async fn get_by_id(State(st): State<Shared>, Path(dept_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dept:query") {
        return denied;
    }
    let d = query_one(
        &db,
        &format!("SELECT {DEPT_COLS} FROM sys_dept d LEFT JOIN sys_user u ON u.user_id = d.leader WHERE d.dept_id = ?"),
        [dept_id],
    );
    if d.is_null() {
        return resp::fail("部门不存在");
    }
    resp::ok(d)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeptForm {
    pub dept_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub dept_name: String,
    pub dept_category: String,
    pub order_num: i64,
    pub leader: Option<i64>,
    pub phone: String,
    pub email: String,
    pub status: String,
}

impl Default for DeptForm {
    fn default() -> Self {
        DeptForm {
            dept_id: None,
            parent_id: Some(0),
            dept_name: String::new(),
            dept_category: String::new(),
            order_num: 0,
            leader: None,
            phone: String::new(),
            email: String::new(),
            status: "0".into(),
        }
    }
}

/// POST /system/dept
pub async fn add(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: DeptForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if f.dept_name.is_empty() {
        return resp::fail("部门名称不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dept:add") {
        return denied;
    }
    let pid = f.parent_id.unwrap_or(0);
    let parent = query_one(&db, "SELECT ancestors FROM sys_dept WHERE dept_id = ? AND del_flag = '0'", [pid]);
    let ancestors = if parent.is_null() {
        if pid == 0 {
            "0".to_string()
        } else {
            return resp::fail("上级部门不存在");
        }
    } else {
        format!("{},{}", parent["ancestors"].as_str().unwrap_or("0"), pid)
    };
    let id = st.ids.next();
    let _ = db.execute(
        "INSERT INTO sys_dept (dept_id, tenant_id, parent_id, ancestors, dept_name, dept_category, order_num, leader, phone, email, status, del_flag, create_dept, create_by, create_time)
         VALUES (?, '000000', ?, ?, ?, ?, ?, ?, ?, ?, ?, '0', ?, ?, ?)",
        rusqlite::params![id, pid, ancestors, f.dept_name, f.dept_category, f.order_num, f.leader, f.phone, f.email, f.status, user.dept_id, user.user_id, now_str()],
    );
    crate::monitor::log_oper(&st, &db, &user, "部门管理", 1, "POST", "/system/dept", &f.dept_name, 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/dept
pub async fn edit(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: DeptForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(id) = f.dept_id else { return resp::fail("部门ID不能为空") };
    if f.dept_name.is_empty() {
        return resp::fail("部门名称不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dept:edit") {
        return denied;
    }
    let pid = f.parent_id.unwrap_or(0);
    if pid == id {
        return resp::fail("修改部门失败，上级部门不能是自己");
    }
    let parent = query_one(&db, "SELECT ancestors FROM sys_dept WHERE dept_id = ? AND del_flag = '0'", [pid]);
    let ancestors = if parent.is_null() {
        if pid == 0 {
            "0".to_string()
        } else {
            return resp::fail("上级部门不存在");
        }
    } else {
        format!("{},{}", parent["ancestors"].as_str().unwrap_or("0"), pid)
    };
    let new_ancestors = format!("{ancestors},{id}");
    let _ = db.execute(
        "UPDATE sys_dept SET parent_id = ?, ancestors = ?, dept_name = ?, dept_category = ?, order_num = ?, leader = ?, phone = ?, email = ?, status = ?, update_by = ?, update_time = ? WHERE dept_id = ?",
        rusqlite::params![pid, ancestors, f.dept_name, f.dept_category, f.order_num, f.leader, f.phone, f.email, f.status, user.user_id, now_str(), id],
    );
    // cascade ancestors to children (Java: updateDeptChildren, find_in_set semantics)
    let _ = db.execute(
        "UPDATE sys_dept SET ancestors = ? || substr(ancestors, (SELECT LENGTH(ancestors) + 1 FROM sys_dept WHERE dept_id = ?))
         WHERE (ancestors LIKE ? OR ancestors LIKE ?) AND dept_id != ?",
        rusqlite::params![
            new_ancestors,
            id,
            format!("%{id},%"),
            format!("%{id}"),
            id
        ],
    );
    crate::monitor::log_oper(&st, &db, &user, "部门管理", 2, "PUT", "/system/dept", &f.dept_name, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// DELETE /system/dept/{deptId}
pub async fn remove(State(st): State<Shared>, Path(dept_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dept:remove") {
        return denied;
    }
    let child = query_one(&db, "SELECT 1 AS ok FROM sys_dept WHERE parent_id = ? AND del_flag = '0' LIMIT 1", [dept_id]);
    if !child.is_null() {
        return resp::fail("存在下级部门，不允许删除");
    }
    let used = query_one(&db, "SELECT 1 AS ok FROM sys_user WHERE dept_id = ? AND del_flag = '0' LIMIT 1", [dept_id]);
    if !used.is_null() {
        return resp::fail("部门存在用户，不允许删除");
    }
    let _ = db.execute("UPDATE sys_dept SET del_flag = '2', update_by = ?, update_time = ? WHERE dept_id = ?", rusqlite::params![user.user_id, now_str(), dept_id]);
    crate::monitor::log_oper(&st, &db, &user, "部门管理", 3, "DELETE", "/system/dept", &dept_id.to_string(), 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}
