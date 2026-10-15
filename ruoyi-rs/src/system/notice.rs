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
pub struct NoticeQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub notice_title: String,
    pub create_by: String,
    pub notice_type: String,
}

impl Default for NoticeQuery {
    fn default() -> Self {
        NoticeQuery {
            page: PageQuery::default(),
            notice_title: String::new(),
            create_by: String::new(),
            notice_type: String::new(),
        }
    }
}

const NOTICE_COLS: &str =
    "notice_id, tenant_id, notice_title, notice_type, notice_content, status, create_dept, create_by, create_time, update_by, update_time, remark";

/// GET /system/notice/list
pub async fn list(State(st): State<Shared>, Query(q): Query<NoticeQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:notice:list") {
        return denied.into();
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.notice_title.is_empty() {
        cond.push_str(" AND notice_title LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.notice_title));
    }
    if !q.create_by.is_empty() {
        cond.push_str(" AND create_by = ?");
        params.push(q.create_by.clone());
    }
    if !q.notice_type.is_empty() {
        cond.push_str(" AND notice_type = ?");
        params.push(q.notice_type.clone());
    }
    let refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let total: i64 = db
        .query_row(&format!("SELECT COUNT(*) FROM sys_notice WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        &db,
        &format!("SELECT {NOTICE_COLS} FROM sys_notice WHERE {cond} ORDER BY notice_id{}", q.page.limit_sql()),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// GET /system/notice/{noticeId}
pub async fn get_by_id(State(st): State<Shared>, Path(notice_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:notice:query") {
        return denied;
    }
    let n = query_one(&db, &format!("SELECT {NOTICE_COLS} FROM sys_notice WHERE notice_id = ?"), [notice_id]);
    if n.is_null() {
        return resp::fail("公告不存在");
    }
    resp::ok(n)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NoticeForm {
    pub notice_id: Option<i64>,
    pub notice_title: String,
    pub notice_type: String,
    pub notice_content: String,
    pub status: String,
    pub remark: String,
}

impl Default for NoticeForm {
    fn default() -> Self {
        NoticeForm {
            notice_id: None,
            notice_title: String::new(),
            notice_type: "1".into(),
            notice_content: String::new(),
            status: "0".into(),
            remark: String::new(),
        }
    }
}

/// POST /system/notice
pub async fn add(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: NoticeForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if f.notice_title.is_empty() {
        return resp::fail("公告标题不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:notice:add") {
        return denied;
    }
    let id = st.ids.next();
    if db
        .execute(
            "INSERT INTO sys_notice (notice_id, tenant_id, notice_title, notice_type, notice_content, status, create_dept, create_by, create_time, remark)
             VALUES (?, '000000', ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![id, f.notice_title, f.notice_type, f.notice_content, f.status, user.dept_id, user.user_id, now_str(), f.remark],
        )
        .is_err()
    {
        return resp::fail("新增公告失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "通知公告", 1, "POST", "/system/notice", &f.notice_title, 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/notice
pub async fn edit(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: NoticeForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(id) = f.notice_id else { return resp::fail("公告ID不能为空") };
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:notice:edit") {
        return denied;
    }
    if db
        .execute(
            "UPDATE sys_notice SET notice_title = ?, notice_type = ?, notice_content = ?, status = ?, remark = ?, update_by = ?, update_time = ? WHERE notice_id = ?",
            rusqlite::params![f.notice_title, f.notice_type, f.notice_content, f.status, f.remark, user.user_id, now_str(), id],
        )
        .is_err()
    {
        return resp::fail("修改公告失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "通知公告", 2, "PUT", "/system/notice", &f.notice_title, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// DELETE /system/notice/{noticeIds}
pub async fn remove(State(st): State<Shared>, Path(notice_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:notice:remove") {
        return denied;
    }
    for id in notice_ids.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        let _ = db.execute("DELETE FROM sys_notice WHERE notice_id = ?", [id]);
    }
    crate::monitor::log_oper(&st, &db, &user, "通知公告", 3, "DELETE", "/system/notice", &notice_ids, 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}
