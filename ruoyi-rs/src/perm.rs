use crate::{jwt, resp};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::collections::HashSet;

/// Authenticated principal, extracted from `Authorization: Bearer <jwt>`.
#[derive(Clone)]
#[allow(dead_code)] // client_id/dept_id kept for parity with the Java LoginUser
pub struct LoginUser {
    pub token_id: String,
    pub user_id: i64,
    pub user_name: String,
    pub dept_id: i64,
    pub client_id: String,
}

impl<S> FromRequestParts<S> for LoginUser
where
    S: AsRef<crate::db::AppState> + Send + Sync,
{
    type Rejection = resp::Rv;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state: &crate::db::AppState = state.as_ref();
        let header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let token = header.strip_prefix("Bearer ").unwrap_or(&header).trim();
        if token.is_empty() {
            return Err(resp::unauthorized("认证失败，无法访问系统资源"));
        }
        let claims = jwt::verify(&state.cfg.jwt_secret, token)
            .ok_or_else(|| resp::unauthorized("认证失败，无法访问系统资源"))?;
        {
            let sessions = state.sessions.lock().await;
            if !sessions.contains_key(&claims.tid) {
                return Err(resp::unauthorized("认证失败，无法访问系统资源"));
            }
        }
        Ok(LoginUser {
            token_id: claims.tid,
            user_id: claims.uid,
            user_name: claims.uname,
            dept_id: 0,
            client_id: claims.cid,
        })
    }
}

/// menu perms for a user; superadmin (user_id 1) gets the wildcard like the
/// Java SysPermissionServiceImpl.
pub fn load_perms(db: &rusqlite::Connection, user_id: i64) -> HashSet<String> {
    if user_id == 1 {
        return HashSet::from(["*:*:*".to_string()]);
    }
    let sql = "SELECT DISTINCT m.perms FROM sys_user_role ur
        JOIN sys_role r ON r.role_id = ur.role_id AND r.del_flag = '0' AND r.status = '0'
        JOIN sys_role_menu rm ON rm.role_id = r.role_id
        JOIN sys_menu m ON m.menu_id = rm.menu_id AND m.status = '0'
        WHERE ur.user_id = ?";
    let mut set = HashSet::new();
    if let Ok(mut stmt) = db.prepare(sql) {
        if let Ok(rows) = stmt.query_map([user_id], |r| r.get::<_, String>(0)) {
            for p in rows.flatten() {
                if !p.is_empty() {
                    set.insert(p);
                }
            }
        }
    }
    set
}

/// role keys; superadmin gets "superadmin" injected (Java behavior).
pub fn load_role_keys(db: &rusqlite::Connection, user_id: i64) -> HashSet<String> {
    if user_id == 1 {
        return HashSet::from(["superadmin".to_string()]);
    }
    let sql = "SELECT DISTINCT r.role_key FROM sys_user_role ur
        JOIN sys_role r ON r.role_id = ur.role_id AND r.del_flag = '0' AND r.status = '0'
        WHERE ur.user_id = ?";
    let mut set = HashSet::new();
    if let Ok(mut stmt) = db.prepare(sql) {
        if let Ok(rows) = stmt.query_map([user_id], |r| r.get::<_, String>(0)) {
            for p in rows.flatten() {
                if !p.is_empty() {
                    set.insert(p);
                }
            }
        }
    }
    set
}

/// None = permitted; Some(response) = denied.
pub fn check(user: &LoginUser, perms: &HashSet<String>, need: &str) -> Option<resp::Rv> {
    if perms.contains("*:*:*") || perms.contains(need) {
        None
    } else {
        Some(resp::forbidden(&format!(
            "没有访问权限，请联系管理员授权: {need} (user {})",
            user.user_id
        )))
    }
}

/// Minimal User-Agent parsing for logininfor records.
pub fn browser_os(ua: &str) -> (String, String) {
    let browser = if ua.contains("Edg") {
        "Edge"
    } else if ua.contains("Chrome") {
        "Chrome"
    } else if ua.contains("Firefox") {
        "Firefox"
    } else if ua.contains("Safari") {
        "Safari"
    } else {
        "未知"
    };
    let os = if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Mac OS") {
        "macOS"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iOS") {
        "iOS"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "未知"
    };
    (browser.to_string(), os.to_string())
}
