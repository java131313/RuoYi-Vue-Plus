use crate::db::Shared;
use crate::perm::{self, LoginUser};
use crate::resp::{self, Rv};
use crate::util::{like_wrap, now_str, query_json, query_one};
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{json, Map, Value};

const MENU_COLS: &str = "menu_id, menu_name, parent_id, order_num, path, component, query_param, is_frame, is_cache, menu_type, visible, status, perms, icon, create_dept, create_by, create_time, update_by, update_time, remark";
/// Same columns qualified for JOINs (unqualified names are ambiguous with sys_role_menu).
const MENU_COLS_M: &str = "m.menu_id, m.menu_name, m.parent_id, m.order_num, m.path, m.component, m.query_param, m.is_frame, m.is_cache, m.menu_type, m.visible, m.status, m.perms, m.icon, m.create_dept, m.create_by, m.create_time, m.update_by, m.update_time, m.remark";

fn strv(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn is_http(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn capitalize(s: &str) -> String {
    let mut cs = s.chars();
    match cs.next() {
        Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}

fn inner_link_replace(path: &str) -> String {
    path.replace("http://", "").replace("https://", "").replace("www.", "").replace('.', "/").replace(':', "/")
}

/// Java SysMenu.getRouteName / getRouterPath / getComponentInfo / isMenuFrame / isInnerLink.
struct MenuLogic<'a> {
    m: &'a Value,
}

impl<'a> MenuLogic<'a> {
    fn parent_id(&self) -> i64 {
        self.m["parentId"].as_i64().unwrap_or(0)
    }
    fn menu_type(&self) -> &str {
        strv(&self.m["menuType"])
    }
    fn is_frame(&self) -> &str {
        // stored as integer or text depending on affinity
        self.m["isFrame"].as_str().unwrap_or_else(|| match self.m["isFrame"].as_i64() {
            Some(1) => "1",
            _ => "0",
        })
    }
    fn is_cache(&self) -> &str {
        self.m["isCache"].as_str().unwrap_or_else(|| match self.m["isCache"].as_i64() {
            Some(1) => "1",
            _ => "0",
        })
    }
    fn path(&self) -> &str {
        strv(&self.m["path"])
    }
    fn is_menu_frame(&self) -> bool {
        self.parent_id() == 0 && self.menu_type() == "C" && self.is_frame() == "1"
    }
    fn is_inner_link(&self) -> bool {
        self.is_frame() == "1" && is_http(self.path())
    }
    fn is_parent_view(&self) -> bool {
        self.parent_id() != 0 && self.menu_type() == "M"
    }
    fn route_name(&self) -> String {
        if self.is_menu_frame() {
            String::new()
        } else {
            capitalize(self.path())
        }
    }
    fn router_path(&self) -> String {
        let mut p = self.path().to_string();
        if self.parent_id() != 0 && self.is_inner_link() {
            p = inner_link_replace(&p);
        }
        if self.parent_id() == 0 && self.menu_type() == "M" && self.is_frame() == "1" {
            p = format!("/{p}");
        } else if self.is_menu_frame() {
            p = "/".into();
        }
        p
    }
    fn component_info(&self) -> String {
        let component = strv(&self.m["component"]);
        if !component.is_empty() && !self.is_menu_frame() {
            component.to_string()
        } else if component.is_empty() && self.parent_id() != 0 && self.is_inner_link() {
            "InnerLink".into()
        } else if component.is_empty() && self.is_parent_view() {
            "ParentView".into()
        } else {
            "Layout".into()
        }
    }
}

fn meta_vo(menu: &Value, with_link: bool, with_active: bool) -> Value {
    let path = strv(&menu["path"]);
    let remark = strv(&menu["remark"]);
    let logic = MenuLogic { m: menu };
    let mut meta = Map::new();
    meta.insert("title".into(), menu["menuName"].clone());
    meta.insert("icon".into(), menu["icon"].clone());
    meta.insert("noCache".into(), json!(logic.is_cache() == "1"));
    if with_link {
        meta.insert("link".into(), if is_http(path) { json!(path) } else { Value::Null });
    }
    if with_active {
        let active = if remark.starts_with('/') { json!(remark) } else { Value::Null };
        meta.insert("activeMenu".into(), active);
    }
    Value::Object(meta)
}

/// Faithful port of SysMenuServiceImpl.buildMenus.
fn build_menus(menus: &[Value]) -> Vec<Value> {
    let mut routers = Vec::new();
    for menu in menus {
        let logic = MenuLogic { m: menu };
        let menu_id = menu["menuId"].as_i64().unwrap_or(0);
        let menu_name = menu["menuName"].clone();
        let query = menu["queryParam"].clone();
        let children = menu["children"].as_array().cloned().unwrap_or_default();

        let mut router = Map::new();
        router.insert("name".into(), json!(format!("{}{menu_id}", logic.route_name())));
        router.insert("path".into(), json!(logic.router_path()));
        router.insert("hidden".into(), json!(strv(&menu["visible"]) == "1"));
        router.insert("component".into(), json!(logic.component_info()));
        router.insert("query".into(), query.clone());

        if !children.is_empty() && logic.menu_type() == "M" {
            router.insert("alwaysShow".into(), json!(true));
            router.insert("redirect".into(), json!("noRedirect"));
            router.insert("meta".into(), meta_vo(menu, true, true));
            router.insert("children".into(), json!(build_menus(&children)));
        } else if logic.is_menu_frame() {
            router.insert("meta".into(), Value::Null);
            let mut child = Map::new();
            child.insert("path".into(), menu["path"].clone());
            child.insert("component".into(), menu["component"].clone());
            child.insert("name".into(), json!(format!("{}{menu_id}", capitalize(strv(&menu["path"])))));
            child.insert("meta".into(), meta_vo(menu, true, false));
            child.insert("query".into(), query);
            router.insert("children".into(), json!([Value::Object(child)]));
        } else if logic.parent_id() == 0 && logic.is_inner_link() {
            router.insert("meta".into(), meta_vo_simple(&menu_name, &menu["icon"]));
            router.insert("path".into(), json!("/"));
            let router_path = inner_link_replace(logic.path());
            let mut child = Map::new();
            child.insert("path".into(), json!(router_path));
            child.insert("component".into(), json!("InnerLink"));
            child.insert("name".into(), json!(format!("{}{menu_id}", capitalize(&router_path))));
            child.insert("meta".into(), meta_vo_simple(&menu_name, &menu["icon"]));
            router.insert("children".into(), json!([Value::Object(child)]));
        } else {
            router.insert("meta".into(), meta_vo(menu, true, true));
        }
        routers.push(Value::Object(router));
    }
    routers
}

fn meta_vo_simple(title: &Value, icon: &Value) -> Value {
    let mut meta = Map::new();
    meta.insert("title".into(), title.clone());
    meta.insert("icon".into(), icon.clone());
    meta.insert("noCache".into(), Value::Null);
    meta.insert("link".into(), Value::Null);
    meta.insert("activeMenu".into(), Value::Null);
    Value::Object(meta)
}

/// Menus visible to a user: M+C, status normal, role-filtered for non-admin.
fn user_menus(db: &rusqlite::Connection, user_id: i64) -> Vec<Value> {
    let sql = if user_id == 1 {
        format!(
            "SELECT {MENU_COLS} FROM sys_menu WHERE menu_type IN ('M','C') AND status = '0' ORDER BY parent_id, order_num"
        )
    } else {
        format!(
            "SELECT DISTINCT {MENU_COLS_M} FROM sys_menu m
             JOIN sys_role_menu rm ON rm.menu_id = m.menu_id
             JOIN sys_user_role ur ON ur.role_id = rm.role_id
             JOIN sys_role r ON r.role_id = ur.role_id AND r.del_flag = '0' AND r.status = '0'
             WHERE m.menu_type IN ('M','C') AND m.status = '0' AND ur.user_id = {user_id}
             ORDER BY m.parent_id, m.order_num"
        )
    };
    query_json(db, &sql, rusqlite::params![])
}

/// GET /system/menu/getRouters
pub async fn get_routers(State(st): State<Shared>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let flat = user_menus(&db, user.user_id);
    let tree = nest(flat, 0);
    resp::ok(serde_json::to_value(build_menus(&tree)).unwrap())
}

/// Nest flat menu rows under parent 0 (RuoYi getChildPerms).
fn nest(mut flat: Vec<Value>, root: i64) -> Vec<Value> {
    fn children_of(flat: &[Value], parent_id: i64) -> Vec<Value> {
        flat.iter()
            .filter(|m| m["parentId"].as_i64().unwrap_or(-1) == parent_id)
            .cloned()
            .map(|mut m| {
                let id = m["menuId"].as_i64().unwrap_or(0);
                let kids = children_of(flat, id);
                m.as_object_mut().unwrap().insert("children".into(), json!(kids));
                m
            })
            .collect()
    }
    let out = children_of(&flat, root);
    flat.clear();
    out
}

/// GET /system/menu/list — flat list (not paged), filtered.
pub async fn list(State(st): State<Shared>, Query(q): Query<std::collections::HashMap<String, String>>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:menu:list") {
        return denied;
    }
    let mut cond = "1 = 1".to_string();
    let mut params: Vec<String> = Vec::new();
    let keywords = q.get("keywords").cloned().unwrap_or_default();
    let menu_name = if keywords.is_empty() { q.get("menuName").cloned().unwrap_or_default() } else { keywords };
    if !menu_name.is_empty() {
        cond.push_str(" AND menu_name LIKE ? ESCAPE '\\'");
        params.push(like_wrap(&menu_name));
    }
    if let Some(v) = q.get("visible").filter(|s| !s.is_empty()) {
        cond.push_str(" AND visible = ?");
        params.push(v.clone());
    }
    if let Some(v) = q.get("status").filter(|s| !s.is_empty()) {
        cond.push_str(" AND status = ?");
        params.push(v.clone());
    }
    if let Some(v) = q.get("menuType").filter(|s| !s.is_empty()) {
        cond.push_str(" AND menu_type = ?");
        params.push(v.clone());
    }
    if let Some(v) = q.get("parentId").and_then(|s| s.parse::<i64>().ok()) {
        cond.push_str(&format!(" AND parent_id = {v}"));
    }
    if user.user_id != 1 {
        cond.push_str(&format!(
            " AND menu_id IN (SELECT rm.menu_id FROM sys_role_menu rm JOIN sys_user_role ur ON ur.role_id = rm.role_id WHERE ur.user_id = {})",
            user.user_id
        ));
    }
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
    let rows = query_json(
        &db,
        &format!("SELECT {MENU_COLS} FROM sys_menu WHERE {cond} ORDER BY parent_id, order_num"),
        refs.as_slice(),
    );
    resp::ok(serde_json::to_value(rows).unwrap())
}

/// GET /system/menu/{menuId}
pub async fn get_by_id(State(st): State<Shared>, Path(menu_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:menu:query") {
        return denied;
    }
    let m = query_one(&db, &format!("SELECT {MENU_COLS} FROM sys_menu WHERE menu_id = ?"), [menu_id]);
    if m.is_null() {
        return resp::fail("菜单不存在");
    }
    resp::ok(m)
}

/// GET /system/menu/treeselect — full tree for role-form menu picker.
pub async fn treeselect(State(st): State<Shared>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:menu:list") {
        return denied;
    }
    let rows = query_json(&db, &format!("SELECT {MENU_COLS} FROM sys_menu ORDER BY parent_id, order_num"), rusqlite::params![]);
    resp::ok(serde_json::to_value(build_tree_extras(rows)).unwrap())
}

fn build_tree_extras(rows: Vec<Value>) -> Value {
    let rows: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let mut obj = r.as_object().cloned().unwrap_or_default();
            let id = obj.remove("menuId").unwrap_or(Value::Null);
            let pid = obj.remove("parentId").unwrap_or(Value::Null);
            let w = obj.remove("orderNum").unwrap_or(json!(0));
            let label = obj.get("menuName").cloned().unwrap_or(Value::Null);
            let mut out = Map::new();
            out.insert("id".into(), id);
            out.insert("parentId".into(), pid);
            out.insert("weight".into(), w);
            out.insert("label".into(), label);
            for k in ["menuType", "icon", "visible", "status", "perms"] {
                if let Some(v) = obj.get(k) {
                    out.insert(k.to_string(), v.clone());
                }
            }
            Value::Object(out)
        })
        .collect();
    crate::system::build_tree(rows, "label", &["menuType", "icon", "visible", "status", "perms"])
}

/// GET /system/menu/roleMenuTreeselect/{roleId}
pub async fn role_menu_treeselect(State(st): State<Shared>, Path(role_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:menu:list") {
        return denied;
    }
    let rows = query_json(&db, &format!("SELECT {MENU_COLS} FROM sys_menu ORDER BY parent_id, order_num"), rusqlite::params![]);
    // checkedKeys excludes parent ids when menuCheckStrictly (Java mapper logic)
    let checked: Vec<i64> = {
        let sql = "SELECT rm.menu_id FROM sys_role_menu rm
            WHERE rm.role_id = ? AND rm.menu_id NOT IN (
                SELECT m.parent_id FROM sys_menu m
                JOIN sys_role_menu rm2 ON rm2.menu_id = m.menu_id
                WHERE rm2.role_id = ? AND m.parent_id != 0
            )";
        let mut out = Vec::new();
        if let Ok(mut stmt) = db.prepare(sql) {
            if let Ok(rs) = stmt.query_map(rusqlite::params![role_id, role_id], |r| r.get::<_, i64>(0)) {
                out.extend(rs.flatten());
            }
        }
        out
    };
    resp::ok(json!({
        "menus": build_tree_extras(rows),
        "checkedKeys": checked,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MenuForm {
    pub menu_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub menu_name: String,
    pub order_num: i64,
    pub path: String,
    pub component: String,
    pub query_param: String,
    pub is_frame: String,
    pub is_cache: String,
    pub menu_type: String,
    pub visible: String,
    pub status: String,
    pub perms: String,
    pub icon: String,
    pub remark: String,
}

impl Default for MenuForm {
    fn default() -> Self {
        MenuForm {
            menu_id: None,
            parent_id: Some(0),
            menu_name: String::new(),
            order_num: 0,
            path: String::new(),
            component: String::new(),
            query_param: String::new(),
            is_frame: "1".into(),
            is_cache: "0".into(),
            menu_type: String::new(),
            visible: "0".into(),
            status: "0".into(),
            perms: String::new(),
            icon: "#".into(),
            remark: String::new(),
        }
    }
}

impl MenuForm {
    // plus-ui sends `query` in some versions, `queryParam` in others
    fn from_json(body: &str) -> Result<Self, serde_json::Error> {
        let mut v: Value = serde_json::from_str(body)?;
        if let Some(q) = v.as_object_mut() {
            if !q.contains_key("queryParam") {
                if let Some(x) = q.remove("query") {
                    q.insert("queryParam".into(), x);
                }
            }
        }
        serde_json::from_value(v)
    }
}

/// POST /system/menu
pub async fn add(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f = match MenuForm::from_json(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    if f.menu_name.is_empty() {
        return resp::fail("菜单名称不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:menu:add") {
        return denied;
    }
    let mid = st.ids.next();
    let _ = db.execute(
        "INSERT INTO sys_menu (menu_id, menu_name, parent_id, order_num, path, component, query_param, is_frame, is_cache, menu_type, visible, status, perms, icon, create_dept, create_by, create_time, remark)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        rusqlite::params![
            mid,
            f.menu_name,
            f.parent_id.unwrap_or(0),
            f.order_num,
            f.path,
            f.component,
            f.query_param,
            f.is_frame,
            f.is_cache,
            f.menu_type,
            f.visible,
            f.status,
            f.perms,
            f.icon,
            user.dept_id,
            user.user_id,
            now_str(),
            f.remark
        ],
    );
    crate::monitor::log_oper(&st, &db, &user, "菜单管理", 1, "POST", "/system/menu", &f.menu_name, 0, "", 0);
    resp::ok_msg(json!({}), "添加成功")
}

/// PUT /system/menu
pub async fn edit(State(st): State<Shared>, user: LoginUser, body: String) -> Rv {
    let f = match MenuForm::from_json(&body) {
        Ok(v) => v,
        Err(_) => return resp::fail("请求参数格式错误"),
    };
    let Some(mid) = f.menu_id else { return resp::fail("菜单ID不能为空") };
    if f.menu_name.is_empty() {
        return resp::fail("菜单名称不能为空");
    }
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:menu:edit") {
        return denied;
    }
    if mid == f.parent_id.unwrap_or(0) {
        return resp::fail("修改菜单失败，上级菜单不能选择自己");
    }
    let _ = db.execute(
        "UPDATE sys_menu SET menu_name = ?, parent_id = ?, order_num = ?, path = ?, component = ?, query_param = ?, is_frame = ?, is_cache = ?, menu_type = ?, visible = ?, status = ?, perms = ?, icon = ?, update_by = ?, update_time = ?, remark = ? WHERE menu_id = ?",
        rusqlite::params![
            f.menu_name,
            f.parent_id.unwrap_or(0),
            f.order_num,
            f.path,
            f.component,
            f.query_param,
            f.is_frame,
            f.is_cache,
            f.menu_type,
            f.visible,
            f.status,
            f.perms,
            f.icon,
            user.user_id,
            now_str(),
            f.remark,
            mid
        ],
    );
    crate::monitor::log_oper(&st, &db, &user, "菜单管理", 2, "PUT", "/system/menu", &f.menu_name, 0, "", 0);
    resp::ok_msg(json!({}), "修改成功")
}

/// DELETE /system/menu/{menuId}
pub async fn remove(State(st): State<Shared>, Path(menu_id): Path<i64>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:menu:remove") {
        return denied;
    }
    let child = query_one(&db, "SELECT 1 AS ok FROM sys_menu WHERE parent_id = ? LIMIT 1", [menu_id]);
    if !child.is_null() {
        return resp::fail("存在子菜单，不允许删除");
    }
    let _ = db.execute("DELETE FROM sys_menu WHERE menu_id = ?", [menu_id]);
    let _ = db.execute("DELETE FROM sys_role_menu WHERE menu_id = ?", [menu_id]);
    crate::monitor::log_oper(&st, &db, &user, "菜单管理", 3, "DELETE", "/system/menu", &menu_id.to_string(), 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}

/// DELETE /system/menu/cascade/{menuIds} — delete a menu and its subtree.
pub async fn cascade_remove(State(st): State<Shared>, Path(menu_ids): Path<String>, user: LoginUser) -> Rv {
    let db = st.db.lock().await;
    let perms = perm::load_perms(&db, user.user_id);
    if let Some(denied) = perm::check(&user, &perms, "system:menu:remove") {
        return denied;
    }
    for id in menu_ids.split(',').filter_map(|s| s.parse::<i64>().ok()) {
        let rows = query_json(&db, "WITH RECURSIVE sub(menu_id) AS (SELECT ? UNION ALL SELECT m.menu_id FROM sys_menu m JOIN sub s ON m.parent_id = s.menu_id) SELECT menu_id FROM sub", [id]);
        for r in rows {
            if let Some(mid) = r["menuId"].as_i64() {
                let _ = db.execute("DELETE FROM sys_menu WHERE menu_id = ?", [mid]);
                let _ = db.execute("DELETE FROM sys_role_menu WHERE menu_id = ?", [mid]);
            }
        }
    }
    crate::monitor::log_oper(&st, &db, &user, "菜单管理", 3, "DELETE", "/system/menu/cascade", &menu_ids, 0, "", 0);
    resp::ok_msg(json!({}), "删除成功")
}
