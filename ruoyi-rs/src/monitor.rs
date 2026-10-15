use crate::db::Shared;
use crate::pager::PageQuery;
use crate::perm::{self, LoginUser};
use crate::resp::{self, Rv};
use crate::util::{like_wrap, now_str, query_json};
use axum::extract::{Path, Query, State};
use rusqlite::ToSql;
use serde_json::{json, Value};

/// Write a sys_oper_log row (Java: @Log annotation handler).
pub fn log_oper(
    st: &Shared,
    db: &rusqlite::Connection,
    user: &LoginUser,
    title: &str,
    business_type: i64,
    method: &str,
    url: &str,
    oper_param: &str,
    status: i64,
    error_msg: &str,
    cost_time: i64,
) {
    let _ = db.execute(
        "INSERT INTO sys_oper_log (oper_id, tenant_id, title, business_type, method, request_method, operator_type, oper_name, dept_name, oper_url, oper_ip, oper_location, oper_param, json_result, status, error_msg, oper_time, cost_time)
         VALUES (?, '000000', ?, ?, ?, ?, 1, ?, '', ?, '', '', ?, '', ?, ?, ?, ?)",
        rusqlite::params![
            st.ids.next(),
            title,
            business_type,
            method,
            method,
            user.user_name,
            url,
            &oper_param.chars().take(2000).collect::<String>(),
            status,
            &error_msg.chars().take(2000).collect::<String>(),
            now_str(),
            cost_time
        ],
    );
}

/// GET /monitor/online/list — from the in-memory session table.
pub async fn online_list(State(st): State<Shared>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:online:list") {
        return denied.into();
    }
    let sessions = st.sessions.lock().await;
    let rows: Vec<Value> = sessions
        .values()
        .map(|s| {
            json!({
                "tokenId": s.token_id,
                "userName": s.user_name,
                "deptName": s.dept_name,
                "ipaddr": s.ip,
                "loginLocation": "",
                "browser": s.browser,
                "os": s.os,
                "loginTime": s.login_time,
            })
        })
        .collect();
    let total = rows.len() as i64;
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// DELETE /monitor/online/{tokenId} — kick a user.
pub async fn online_kick(State(st): State<Shared>, Path(token_id): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:online:forceLogout") {
        return denied;
    }
    st.sessions.lock().await.remove(&token_id);
    resp::ok_msg(json!({}), "强退成功")
}

/// DELETE /monitor/online/myself/{tokenId}
pub async fn online_kick_myself(State(st): State<Shared>, Path(token_id): Path<String>, user: LoginUser) -> Rv {
    let _ = user;
    st.sessions.lock().await.remove(&token_id);
    resp::ok_msg(json!({}), "退出成功")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LogininforQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub user_name: String,
    pub status: String,
    pub ipaddr: String,
    pub begin_time: String,
    pub end_time: String,
}

impl Default for LogininforQuery {
    fn default() -> Self {
        LogininforQuery {
            page: PageQuery::default(),
            user_name: String::new(),
            status: String::new(),
            ipaddr: String::new(),
            begin_time: String::new(),
            end_time: String::new(),
        }
    }
}

use serde::Deserialize;

/// GET /monitor/logininfor/list
pub async fn logininfor_list(State(st): State<Shared>, Query(q): Query<LogininforQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:logininfor:list") {
        return denied.into();
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.user_name.is_empty() {
        cond.push_str(" AND user_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.user_name));
    }
    if !q.status.is_empty() {
        cond.push_str(" AND status = ?");
        params.push(q.status.clone());
    }
    if !q.ipaddr.is_empty() {
        cond.push_str(" AND ipaddr LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.ipaddr));
    }
    if !q.begin_time.is_empty() {
        cond.push_str(" AND substr(login_time,1,10) >= substr(?,1,10)");
        params.push(q.begin_time.clone());
    }
    if !q.end_time.is_empty() {
        cond.push_str(" AND substr(login_time,1,10) <= substr(?,1,10)");
        params.push(q.end_time.clone());
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let total: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM sys_logininfor WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        &db,
        &format!("SELECT * FROM sys_logininfor WHERE {cond} ORDER BY info_id DESC{}", q.page.limit_sql()),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// DELETE /monitor/logininfor/{infoIds}
pub async fn logininfor_remove(State(st): State<Shared>, Path(info_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:logininfor:remove") {
        return denied;
    }
    for id in info_ids.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        let _ = db.execute("DELETE FROM sys_logininfor WHERE info_id = ?", [id]);
    }
    resp::ok_msg(json!({}), "删除成功")
}

/// DELETE /monitor/logininfor/clean
pub async fn logininfor_clean(State(st): State<Shared>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:logininfor:remove") {
        return denied;
    }
    let _ = db.execute("DELETE FROM sys_logininfor", rusqlite::params![]);
    resp::ok_msg(json!({}), "清空成功")
}

/// GET /monitor/logininfor/unlock/{userName}
pub async fn logininfor_unlock(State(st): State<Shared>, Path(user_name): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:logininfor:unlock") {
        return denied;
    }
    st.pwd_fails.lock().await.remove(&user_name);
    resp::ok_msg(json!({}), &format!("用户 {user_name} 解锁成功"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OperlogQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub title: String,
    pub oper_name: String,
    pub business_type: Option<i64>,
    pub status: Option<i64>,
    pub begin_time: String,
    pub end_time: String,
}

impl Default for OperlogQuery {
    fn default() -> Self {
        OperlogQuery {
            page: PageQuery::default(),
            title: String::new(),
            oper_name: String::new(),
            business_type: None,
            status: None,
            begin_time: String::new(),
            end_time: String::new(),
        }
    }
}

/// GET /monitor/operlog/list
pub async fn operlog_list(State(st): State<Shared>, Query(q): Query<OperlogQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:operlog:list") {
        return denied.into();
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.title.is_empty() {
        cond.push_str(" AND title LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.title));
    }
    if !q.oper_name.is_empty() {
        cond.push_str(" AND oper_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.oper_name));
    }
    if let Some(bt) = q.business_type {
        cond.push_str(" AND business_type = ?");
        params.push(bt.to_string());
    }
    if let Some(s) = q.status {
        cond.push_str(" AND status = ?");
        params.push(s.to_string());
    }
    if !q.begin_time.is_empty() {
        cond.push_str(" AND substr(oper_time,1,10) >= substr(?,1,10)");
        params.push(q.begin_time.clone());
    }
    if !q.end_time.is_empty() {
        cond.push_str(" AND substr(oper_time,1,10) <= substr(?,1,10)");
        params.push(q.end_time.clone());
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let total: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM sys_oper_log WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        &db,
        &format!("SELECT * FROM sys_oper_log WHERE {cond} ORDER BY oper_id DESC{}", q.page.limit_sql()),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// DELETE /monitor/operlog/{operIds}
pub async fn operlog_remove(State(st): State<Shared>, Path(oper_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:operlog:remove") {
        return denied;
    }
    for id in oper_ids.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        let _ = db.execute("DELETE FROM sys_oper_log WHERE oper_id = ?", [id]);
    }
    resp::ok_msg(json!({}), "删除成功")
}

/// DELETE /monitor/operlog/clean
pub async fn operlog_clean(State(st): State<Shared>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "monitor:operlog:remove") {
        return denied;
    }
    let _ = db.execute("DELETE FROM sys_oper_log", rusqlite::params![]);
    resp::ok_msg(json!({}), "清空成功")
}
