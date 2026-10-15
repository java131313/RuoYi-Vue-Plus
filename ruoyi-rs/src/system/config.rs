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
pub struct ConfigQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub config_name: String,
    pub config_key: String,
    pub config_type: String,
    pub status: String,
    pub begin_time: String,
    pub end_time: String,
}

impl Default for ConfigQuery {
    fn default() -> Self {
        ConfigQuery {
            page: PageQuery::default(),
            config_name: String::new(),
            config_key: String::new(),
            config_type: String::new(),
            status: String::new(),
            begin_time: String::new(),
            end_time: String::new(),
        }
    }
}

const CONFIG_COLS: &str =
    "config_id, tenant_id, config_name, config_key, config_value, config_type, create_dept, create_by, create_time, update_by, update_time, remark";

/// GET /system/config/list
pub async fn list(State(st): State<Shared>, Query(q): Query<ConfigQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:config:list") {
        return denied.into();
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.config_name.is_empty() {
        cond.push_str(" AND config_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.config_name));
    }
    if !q.config_key.is_empty() {
        cond.push_str(" AND config_key LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.config_key));
    }
    if !q.config_type.is_empty() {
        cond.push_str(" AND config_type = ?");
        params.push(q.config_type.clone());
    }
    if !q.begin_time.is_empty() {
        cond.push_str(" AND substr(create_time,1,10) >= substr(?,1,10)");
        params.push(q.begin_time.clone());
    }
    if !q.end_time.is_empty() {
        cond.push_str(" AND substr(create_time,1,10) <= substr(?,1,10)");
        params.push(q.end_time.clone());
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let total: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM sys_config WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        &db,
        &format!("SELECT {CONFIG_COLS} FROM sys_config WHERE {cond} ORDER BY config_id{}", q.page.limit_sql()),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// GET /system/config/{configId}
pub async fn get_by_id(State(st): State<Shared>, Path(config_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:config:query") {
        return denied;
    }
    let c = query_one(&db, &format!("SELECT {CONFIG_COLS} FROM sys_config WHERE config_id = ?"), [config_id]);
    if c.is_null() {
        return resp::fail("参数不存在");
    }
    resp::ok(c)
}

/// GET /system/config/configKey/{configKey} — open endpoint.
pub async fn get_by_key(State(st): State<Shared>, Path(config_key): Path<String>) -> Rv {
    let db = st.db.lock().await;
    let c = query_one(
        &db,
        &format!("SELECT {CONFIG_COLS} FROM sys_config WHERE config_key = ?"),
        [config_key],
    );
    if c.is_null() {
        return resp::fail("参数键值不存在");
    }
    resp::ok(c)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigForm {
    pub config_id: Option<i64>,
    pub config_name: String,
    pub config_key: String,
    pub config_value: String,
    pub config_type: String,
    pub status: String,
    pub remark: String,
}

impl Default for ConfigForm {
    fn default() -> Self {
        ConfigForm {
            config_id: None,
            config_name: String::new(),
            config_key: String::new(),
            config_value: String::new(),
            config_type: "N".into(),
            status: "0".into(),
            remark: String::new(),
        }
    }
}

/// POST /system/config
pub async fn add(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: ConfigForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if f.config_name.is_empty() || f.config_key.is_empty() {
        return resp::fail("参数名称和键名不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:config:add") {
        return denied;
    }
    let dup = query_one(&db, "SELECT 1 AS ok FROM sys_config WHERE config_key = ? AND tenant_id = '000000'", [&f.config_key]);
    if !dup.is_null() {
        return resp::fail("新增参数失败，键名已存在");
    }
    let id = st.ids.next();
    if db
        .execute(
            "INSERT INTO sys_config (config_id, tenant_id, config_name, config_key, config_value, config_type, create_dept, create_by, create_time, remark)
             VALUES (?, '000000', ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![id, f.config_name, f.config_key, f.config_value, f.config_type, user.dept_id, user.user_id, now_str(), f.remark],
        )
        .is_err()
    {
        return resp::fail("新增参数失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "参数管理", 1, "POST", "/system/config", &f.config_key, 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/config
pub async fn edit(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: ConfigForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(id) = f.config_id else { return resp::fail("参数ID不能为空") };
    if f.config_name.is_empty() || f.config_key.is_empty() {
        return resp::fail("参数名称和键名不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:config:edit") {
        return denied;
    }
    if db
        .execute(
            "UPDATE sys_config SET config_name = ?, config_key = ?, config_value = ?, config_type = ?, remark = ?, update_by = ?, update_time = ? WHERE config_id = ?",
            rusqlite::params![f.config_name, f.config_key, f.config_value, f.config_type, f.remark, user.user_id, now_str(), id],
        )
        .is_err()
    {
        return resp::fail("修改参数失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "参数管理", 2, "PUT", "/system/config", &f.config_key, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// PUT /system/config/updateByKey
pub async fn update_by_key(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let v: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:config:edit") {
        return denied;
    }
    let config_id = v["configId"].as_i64();
    let config_key = v["configKey"].as_str().unwrap_or("");
    let config_value = v["configValue"].as_str().unwrap_or("");
    let n = if let Some(id) = config_id {
        db.execute(
            "UPDATE sys_config SET config_value = ?, update_by = ?, update_time = ? WHERE config_id = ?",
            rusqlite::params![config_value, user.user_id, now_str(), id],
        )
    } else {
        db.execute(
            "UPDATE sys_config SET config_value = ?, update_by = ?, update_time = ? WHERE config_key = ?",
            rusqlite::params![config_value, user.user_id, now_str(), config_key],
        )
    };
    if n.map(|x| x == 0).unwrap_or(true) {
        return resp::fail("参数不存在");
    }
    crate::monitor::log_oper(&st, &db, &user, "参数管理", 2, "PUT", "/system/config/updateByKey", config_key, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// DELETE /system/config/{configIds}
pub async fn remove(State(st): State<Shared>, Path(config_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:config:remove") {
        return denied;
    }
    for id in config_ids.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        let c = query_one(&db, "SELECT config_type FROM sys_config WHERE config_id = ?", [id]);
        if c["configType"].as_str() == Some("Y") {
            return resp::fail("内置参数，不能删除");
        }
        let _ = db.execute("DELETE FROM sys_config WHERE config_id = ?", [id]);
    }
    crate::monitor::log_oper(&st, &db, &user, "参数管理", 3, "DELETE", "/system/config", &config_ids, 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}

/// DELETE /system/config/refreshCache — no-op (no Redis cache)
pub async fn refresh_cache() -> Rv {
    resp::ok_msg_empty("刷新成功")
}
