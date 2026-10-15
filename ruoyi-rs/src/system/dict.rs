use crate::db::Shared;
use crate::pager::PageQuery;
use crate::perm::{self, LoginUser};
use crate::resp::{self, Rv};
use crate::util::{like_wrap, now_str, query_json, query_one};
use axum::extract::{Path, Query, State};
use rusqlite::ToSql;
use serde::Deserialize;
use serde_json::{json, Value};

// ---------- dict type ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictTypeQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub dict_name: String,
    pub dict_type: String,
    pub status: String,
    pub dict_ids: String,
    pub begin_time: String,
    pub end_time: String,
}

impl Default for DictTypeQuery {
    fn default() -> Self {
        DictTypeQuery {
            page: PageQuery::default(),
            dict_name: String::new(),
            dict_type: String::new(),
            status: String::new(),
            dict_ids: String::new(),
            begin_time: String::new(),
            end_time: String::new(),
        }
    }
}

const TYPE_COLS: &str =
    "dict_id, tenant_id, dict_name, dict_type, create_dept, create_by, create_time, update_by, update_time, remark";

/// GET /system/dict/type/list
pub async fn type_list(State(st): State<Shared>, Query(q): Query<DictTypeQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:list") {
        return denied.into();
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.dict_name.is_empty() {
        cond.push_str(" AND dict_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.dict_name));
    }
    if !q.dict_type.is_empty() {
        cond.push_str(" AND dict_type LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.dict_type));
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
        .query_row(&format!("SELECT COUNT(*) FROM sys_dict_type WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        &db,
        &format!("SELECT {TYPE_COLS} FROM sys_dict_type WHERE {cond} ORDER BY dict_id{}", q.page.limit_sql()),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// GET /system/dict/type/optionselect
pub async fn type_optionselect(State(st): State<Shared>, Query(q): Query<DictTypeQuery>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:list") {
        return denied;
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.dict_ids.is_empty() {
        let ids: Vec<&str> = q.dict_ids.split(',').filter(|s| !s.is_empty()).collect();
        if !ids.is_empty() {
            cond.push_str(&format!(" AND dict_id IN ({})", ids.iter().map(|_| "?").collect::<Vec<_>>().join(",")));
            params.extend(ids.iter().map(|s| s.to_string()));
        }
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let rows = query_json(&db, &format!("SELECT {TYPE_COLS} FROM sys_dict_type WHERE {cond} ORDER BY dict_id"), refs.as_slice());
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/dict/type/{dictId}
pub async fn type_get_by_id(State(st): State<Shared>, Path(dict_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:query") {
        return denied;
    }
    let d = query_one(&db, &format!("SELECT {TYPE_COLS} FROM sys_dict_type WHERE dict_id = ?"), [dict_id]);
    if d.is_null() {
        return resp::fail("字典类型不存在");
    }
    resp::ok(d)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictTypeForm {
    pub dict_id: Option<i64>,
    pub dict_name: String,
    pub dict_type: String,
    pub status: String,
    pub remark: String,
}

impl Default for DictTypeForm {
    fn default() -> Self {
        DictTypeForm {
            dict_id: None,
            dict_name: String::new(),
            dict_type: String::new(),
            status: "0".into(),
            remark: String::new(),
        }
    }
}

/// POST /system/dict/type
pub async fn type_add(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: DictTypeForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if f.dict_name.is_empty() || f.dict_type.is_empty() {
        return resp::fail("字典名称和类型不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:add") {
        return denied;
    }
    let dup = query_one(&db, "SELECT 1 AS ok FROM sys_dict_type WHERE dict_type = ? AND tenant_id = '000000'", [&f.dict_type]);
    if !dup.is_null() {
        return resp::fail("新增字典失败，类型已存在");
    }
    let id = st.ids.next();
    if db
        .execute(
            "INSERT INTO sys_dict_type (dict_id, tenant_id, dict_name, dict_type, create_dept, create_by, create_time, remark) VALUES (?, '000000', ?, ?, ?, ?, ?, ?)",
            rusqlite::params![id, f.dict_name, f.dict_type, user.dept_id, user.user_id, now_str(), f.remark],
        )
        .is_err()
    {
        return resp::fail("新增字典失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "字典类型", 1, "POST", "/system/dict/type", &f.dict_type, 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/dict/type
pub async fn type_edit(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: DictTypeForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(id) = f.dict_id else { return resp::fail("字典ID不能为空") };
    if f.dict_name.is_empty() || f.dict_type.is_empty() {
        return resp::fail("字典名称和类型不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:edit") {
        return denied;
    }
    if db
        .execute(
            "UPDATE sys_dict_type SET dict_name = ?, dict_type = ?, remark = ?, update_by = ?, update_time = ? WHERE dict_id = ?",
            rusqlite::params![f.dict_name, f.dict_type, f.remark, user.user_id, now_str(), id],
        )
        .is_err()
    {
        return resp::fail("修改字典失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "字典类型", 2, "PUT", "/system/dict/type", &f.dict_type, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// DELETE /system/dict/type/{dictIds}
pub async fn type_remove(State(st): State<Shared>, Path(dict_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:remove") {
        return denied;
    }
    for id in dict_ids.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        let t = query_one(&db, "SELECT dict_type FROM sys_dict_type WHERE dict_id = ?", [id]);
        let dict_type = t["dictType"].as_str().unwrap_or("").to_string();
        let used = query_one(&db, "SELECT 1 AS ok FROM sys_dict_data WHERE dict_type = ? LIMIT 1", [&dict_type]);
        if !used.is_null() {
            return resp::fail(&format!("{dict_type}已分配数据，不能删除"));
        }
        let _ = db.execute("DELETE FROM sys_dict_type WHERE dict_id = ?", [id]);
    }
    crate::monitor::log_oper(&st, &db, &user, "字典类型", 3, "DELETE", "/system/dict/type", &dict_ids, 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}

/// DELETE /system/dict/type/refreshCache — no-op (no Redis cache)
pub async fn type_refresh_cache() -> Rv {
    resp::ok_msg_empty("刷新成功")
}

// ---------- dict data ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictDataQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub dict_type: String,
    pub dict_label: String,
    pub status: String,
}

impl Default for DictDataQuery {
    fn default() -> Self {
        DictDataQuery {
            page: PageQuery::default(),
            dict_type: String::new(),
            dict_label: String::new(),
            status: String::new(),
        }
    }
}

const DATA_COLS: &str =
    "dict_code, tenant_id, dict_sort, dict_label, dict_value, dict_type, css_class, list_class, is_default, create_dept, create_by, create_time, update_by, update_time, remark";

/// GET /system/dict/data/type/{dictType} — open endpoint (frontend fetches dicts).
pub async fn data_by_type(State(st): State<Shared>, Path(dict_type): Path<String>) -> Rv {
    let db = st.db.lock().await;
    let rows = query_json(
        &db,
        &format!("SELECT {DATA_COLS} FROM sys_dict_data WHERE dict_type = ? ORDER BY dict_sort"),
        [dict_type],
    );
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/dict/data/list
pub async fn data_list(State(st): State<Shared>, Query(q): Query<DictDataQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:list") {
        return denied.into();
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.dict_type.is_empty() {
        cond.push_str(" AND dict_type = ?");
        params.push(q.dict_type.clone());
    }
    if !q.dict_label.is_empty() {
        cond.push_str(" AND dict_label LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.dict_label));
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let total: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM sys_dict_data WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        &db,
        &format!("SELECT {DATA_COLS} FROM sys_dict_data WHERE {cond} ORDER BY dict_sort{}", q.page.limit_sql()),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// GET /system/dict/data/{dictCode}
pub async fn data_get_by_id(State(st): State<Shared>, Path(dict_code): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:query") {
        return denied;
    }
    let d = query_one(&db, &format!("SELECT {DATA_COLS} FROM sys_dict_data WHERE dict_code = ?"), [dict_code]);
    if d.is_null() {
        return resp::fail("字典数据不存在");
    }
    resp::ok(d)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictDataForm {
    pub dict_code: Option<i64>,
    pub dict_sort: i64,
    pub dict_label: String,
    pub dict_value: String,
    pub dict_type: String,
    pub css_class: String,
    pub list_class: String,
    pub is_default: String,
    pub status: String,
    pub remark: String,
}

impl Default for DictDataForm {
    fn default() -> Self {
        DictDataForm {
            dict_code: None,
            dict_sort: 0,
            dict_label: String::new(),
            dict_value: String::new(),
            dict_type: String::new(),
            css_class: String::new(),
            list_class: String::new(),
            is_default: "N".into(),
            status: "0".into(),
            remark: String::new(),
        }
    }
}

/// POST /system/dict/data
pub async fn data_add(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: DictDataForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if f.dict_label.is_empty() || f.dict_type.is_empty() {
        return resp::fail("字典标签和类型不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:add") {
        return denied;
    }
    let id = st.ids.next();
    if db
        .execute(
            "INSERT INTO sys_dict_data (dict_code, tenant_id, dict_sort, dict_label, dict_value, dict_type, css_class, list_class, is_default, create_dept, create_by, create_time, remark)
             VALUES (?, '000000', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![id, f.dict_sort, f.dict_label, f.dict_value, f.dict_type, f.css_class, f.list_class, f.is_default, user.dept_id, user.user_id, now_str(), f.remark],
        )
        .is_err()
    {
        return resp::fail("新增字典数据失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "字典数据", 1, "POST", "/system/dict/data", &f.dict_label, 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/dict/data
pub async fn data_edit(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: DictDataForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(id) = f.dict_code else { return resp::fail("字典编码不能为空") };
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:edit") {
        return denied;
    }
    if db
        .execute(
            "UPDATE sys_dict_data SET dict_sort = ?, dict_label = ?, dict_value = ?, dict_type = ?, css_class = ?, list_class = ?, is_default = ?, remark = ?, update_by = ?, update_time = ? WHERE dict_code = ?",
            rusqlite::params![f.dict_sort, f.dict_label, f.dict_value, f.dict_type, f.css_class, f.list_class, f.is_default, f.remark, user.user_id, now_str(), id],
        )
        .is_err()
    {
        return resp::fail("修改字典数据失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "字典数据", 2, "PUT", "/system/dict/data", &f.dict_label, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// DELETE /system/dict/data/{dictCodes}
pub async fn data_remove(State(st): State<Shared>, Path(dict_codes): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:dict:remove") {
        return denied;
    }
    for id in dict_codes.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        let _ = db.execute("DELETE FROM sys_dict_data WHERE dict_code = ?", [id]);
    }
    crate::monitor::log_oper(&st, &db, &user, "字典数据", 3, "DELETE", "/system/dict/data", &dict_codes, 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}
