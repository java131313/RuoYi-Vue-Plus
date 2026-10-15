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
pub struct PostQuery {
    #[serde(flatten)]
    pub page: PageQuery,
    pub post_name: String,
    pub post_code: String,
    pub status: String,
    pub begin_time: String,
    pub end_time: String,
}

impl Default for PostQuery {
    fn default() -> Self {
        PostQuery {
            page: PageQuery::default(),
            post_name: String::new(),
            post_code: String::new(),
            status: String::new(),
            begin_time: String::new(),
            end_time: String::new(),
        }
    }
}

const POST_COLS: &str =
    "post_id, tenant_id, dept_id, post_code, post_category, post_name, post_sort, status, create_dept, create_by, create_time, update_by, update_time, remark";

/// GET /system/post/list
pub async fn list(State(st): State<Shared>, Query(q): Query<PostQuery>, user: LoginUser) -> crate::resp::Resp {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:post:list") {
        return denied.into();
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    if !q.post_name.is_empty() {
        cond.push_str(" AND post_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.post_name));
    }
    if !q.post_code.is_empty() {
        cond.push_str(" AND post_code LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&q.post_code));
    }
    if !q.status.is_empty() {
        cond.push_str(" AND status = ?");
        params.push(q.status.clone());
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
        .query_row(&format!("SELECT COUNT(*) FROM sys_post WHERE {cond}"), refs.as_slice(), |r| r.get(0))
        .unwrap_or(0);
    let rows = query_json(
        &db,
        &format!("SELECT {POST_COLS} FROM sys_post WHERE {cond} ORDER BY post_sort{}", q.page.limit_sql()),
        refs.as_slice(),
    );
    crate::resp::TableDataInfo::new(total, Value::Array(rows)).into()
}

/// GET /system/post/optionselect
pub async fn optionselect(State(st): State<Shared>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:post:query") {
        return denied;
    }
    let rows = query_json(&db, &format!("SELECT {POST_COLS} FROM sys_post ORDER BY post_sort"), rusqlite::params![]);
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/post/deptTree — post page dept tree selector
pub async fn dept_tree(State(st): State<Shared>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let rows = query_json(
        &db,
        "SELECT dept_id, parent_id, dept_name, order_num FROM sys_dept WHERE del_flag = '0' ORDER BY parent_id, order_num",
        rusqlite::params![],
    );
    let _ = user;
    resp::ok(serde_json::to_value(crate::system::build_tree_by(rows, "dept_id", "dept_name", &[])).unwrap())
}

/// GET /system/post/{postId}
pub async fn get_by_id(State(st): State<Shared>, Path(post_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:post:query") {
        return denied;
    }
    let p = query_one(&db, &format!("SELECT {POST_COLS} FROM sys_post WHERE post_id = ?"), [post_id]);
    if p.is_null() {
        return resp::fail("岗位不存在");
    }
    resp::ok(p)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PostForm {
    pub post_id: Option<i64>,
    pub dept_id: Option<i64>,
    pub post_name: String,
    pub post_code: String,
    pub post_category: String,
    pub post_sort: i64,
    pub status: String,
    pub remark: String,
}

impl Default for PostForm {
    fn default() -> Self {
        PostForm {
            post_id: None,
            dept_id: None,
            post_name: String::new(),
            post_code: String::new(),
            post_category: String::new(),
            post_sort: 0,
            status: "0".into(),
            remark: String::new(),
        }
    }
}

/// POST /system/post
pub async fn add(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: PostForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if f.post_name.is_empty() || f.post_code.is_empty() {
        return resp::fail("岗位名称和编码不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:post:add") {
        return denied;
    }
    let dup = query_one(&db, "SELECT 1 AS ok FROM sys_post WHERE post_name = ?", [&f.post_name]);
    if !dup.is_null() {
        return resp::fail("新增岗位失败，岗位名称已存在");
    }
    let id = st.ids.next();
    if db
        .execute(
            "INSERT INTO sys_post (post_id, tenant_id, dept_id, post_code, post_category, post_name, post_sort, status, create_dept, create_by, create_time, remark)
             VALUES (?, '000000', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![id, f.dept_id, f.post_code, f.post_category, f.post_name, f.post_sort, f.status, user.dept_id, user.user_id, now_str(), f.remark],
        )
        .is_err()
    {
        return resp::fail("新增岗位失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "岗位管理", 1, "POST", "/system/post", &f.post_name, 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/post
pub async fn edit(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f: PostForm = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(id) = f.post_id else { return resp::fail("岗位ID不能为空") };
    if f.post_name.is_empty() || f.post_code.is_empty() {
        return resp::fail("岗位名称和编码不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:post:edit") {
        return denied;
    }
    if db
        .execute(
            "UPDATE sys_post SET dept_id = ?, post_code = ?, post_category = ?, post_name = ?, post_sort = ?, status = ?, remark = ?, update_by = ?, update_time = ? WHERE post_id = ?",
            rusqlite::params![f.dept_id, f.post_code, f.post_category, f.post_name, f.post_sort, f.status, f.remark, user.user_id, now_str(), id],
        )
        .is_err()
    {
        return resp::fail("修改岗位失败");
    }
    crate::monitor::log_oper(&st, &db, &user, "岗位管理", 2, "PUT", "/system/post", &f.post_name, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// DELETE /system/post/{postIds}
pub async fn remove(State(st): State<Shared>, Path(post_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:post:remove") {
        return denied;
    }
    for id in post_ids.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        let used = query_one(&db, "SELECT 1 AS ok FROM sys_user_post WHERE post_id = ? LIMIT 1", [id]);
        if !used.is_null() {
            return resp::fail("岗位已分配用户，不能删除");
        }
        let _ = db.execute("DELETE FROM sys_post WHERE post_id = ?", [id]);
    }
    crate::monitor::log_oper(&st, &db, &user, "岗位管理", 3, "DELETE", "/system/post", &post_ids, 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}
