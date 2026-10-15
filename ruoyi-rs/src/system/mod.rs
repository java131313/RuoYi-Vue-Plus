pub mod config;
pub mod dept;
pub mod dict;
pub mod menu;
pub mod notice;
pub mod post;
pub mod profile;
pub mod role;
pub mod user;

use serde_json::{json, Map, Value};

/// Build a hutool-style Tree (id/parentId/label/weight/children) from flat rows.
/// Root id follows the first row's parentId, mirroring TreeBuildUtils.
pub fn build_tree(rows: Vec<Value>, label_key: &str, extra_keys: &[&str]) -> Value {
    if rows.is_empty() {
        return json!([]);
    }
    let root_parent = rows[0]["parentId"].clone();
    let mut nodes: Vec<Value> = Vec::new();
    let mut by_id: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let mut m = Map::new();
        m.insert("id".into(), r["id"].clone());
        m.insert("parentId".into(), r["parentId"].clone());
        m.insert("label".into(), r[label_key].clone());
        m.insert("weight".into(), r["weight"].clone());
        for k in extra_keys {
            if let Some(v) = r.get(*k) {
                m.insert((*k).into(), v.clone());
            }
        }
        m.insert("children".into(), json!([]));
        by_id.insert(r["id"].to_string(), i);
        nodes.push(Value::Object(m));
    }
    // second pass: attach to parents
    let mut roots: Vec<Value> = Vec::new();
    let mut attached: Vec<bool> = vec![false; nodes.len()];
    for i in 0..nodes.len() {
        let pid = nodes[i]["parentId"].clone();
        if pid == root_parent || !by_id.contains_key(&pid.to_string()) {
            roots.push(nodes[i].clone());
            attached[i] = true;
        }
    }
    for i in 0..nodes.len() {
        if attached[i] {
            continue;
        }
        let pid = nodes[i]["parentId"].to_string();
        if let Some(&pi) = by_id.get(&pid) {
            let child = std::mem::replace(&mut nodes[i], Value::Null);
            if let Some(children) = nodes[pi]["children"].as_array_mut() {
                children.push(child);
            }
        } else {
            roots.push(std::mem::replace(&mut nodes[i], Value::Null));
        }
    }
    json!(roots)
}

/// Flat rows -> tree via menu_id/parent_id (for menu/dept entities whose
/// id column is not literally "id").
pub fn build_tree_by(rows: Vec<Value>, id_key: &str, label_key: &str, extra_keys: &[&str]) -> Value {
    let rows: Vec<Value> = rows
        .into_iter()
        .map(|mut r| {
            if let Some(obj) = r.as_object_mut() {
                let id = obj.remove(id_key).unwrap_or(Value::Null);
                let pid = obj.remove("parentId").unwrap_or(Value::Null);
                let w = obj.remove("orderNum").unwrap_or(json!(0));
                obj.insert("id".into(), id);
                obj.insert("parentId".into(), pid);
                obj.insert("weight".into(), w);
            }
            r
        })
        .collect();
    build_tree(rows, label_key, extra_keys)
}

/// Data-scope filter for sys_user listings, built from the user's roles
/// (Java: SysDataScopeHelper ORs every role's scope SQL; superadmin skips).
/// Returns a WHERE fragment (without leading AND) or None for "all data".
pub fn data_scope_filter(db: &rusqlite::Connection, user_id: i64, alias: &str) -> Option<String> {
    if user_id == 1 {
        return None;
    }
    let scopes: Vec<i64> = {
        let sql = "SELECT DISTINCT r.data_scope FROM sys_user_role ur
            JOIN sys_role r ON r.role_id = ur.role_id AND r.del_flag = '0'
            WHERE ur.user_id = ?";
        let mut out = Vec::new();
        if let Ok(mut stmt) = db.prepare(sql) {
            if let Ok(rows) = stmt.query_map([user_id], |r| r.get::<_, Option<i64>>(0)) {
                for s in rows.flatten().flatten() {
                    out.push(s);
                }
            }
        }
        out
    };
    let below = format!(
        "{alias}.dept_id IN (SELECT d2.dept_id FROM sys_dept d2 WHERE d2.dept_id = (SELECT dept_id FROM sys_user WHERE user_id = {user_id}) OR d2.ancestors LIKE (SELECT '%' || d1.ancestors || '%' FROM sys_dept d1 WHERE d1.dept_id = (SELECT dept_id FROM sys_user WHERE user_id = {user_id})))"
    );
    let mut frags: Vec<String> = Vec::new();
    for s in scopes {
        match s {
            1 => return None,
            2 => frags.push(format!(
                "{alias}.dept_id IN (SELECT rd.dept_id FROM sys_role_dept rd JOIN sys_user_role ur ON ur.role_id = rd.role_id WHERE ur.user_id = {user_id})"
            )),
            3 => frags.push(format!(
                "{alias}.dept_id = (SELECT dept_id FROM sys_user WHERE user_id = {user_id})"
            )),
            4 => frags.push(below.clone()),
            6 => frags.push(format!("({below} OR {alias}.user_id = {user_id})")),
            _ => frags.push(format!("{alias}.user_id = {user_id}")),
        }
    }
    if frags.is_empty() {
        frags.push(format!("{alias}.user_id = {user_id}"));
    }
    Some(frags.join(" OR "))
}
