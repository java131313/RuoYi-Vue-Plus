mod api;
mod auth;
mod captcha;
mod config;
mod crypto;
mod db;
mod id;
mod jwt;
mod monitor;
mod pager;
mod perm;
mod resp;
mod session;
mod system;
mod util;
mod web;

use axum::routing::{delete, get, post, put};
use axum::Router;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use db::AppState;

fn api_router() -> Router<Arc<AppState>> {
    Router::new()
        // ---- auth (public) ----
        .route("/auth/code", get(auth::code))
        .route("/auth/login", post(auth::login))
        .route("/auth/register", post(auth::register))
        .route("/auth/tenant/list", get(auth::tenant_list))
        .route("/auth/logout", post(auth::logout))
        // ---- system/user ----
        .route("/system/user/list", get(system::user::list))
        .route("/system/user/optionselect", get(system::user::optionselect))
        .route("/system/user/getInfo", get(system::user::get_info))
        .route("/system/user/deptTree", get(system::user::dept_tree))
        .route("/system/user/authRole/{userId}", get(system::user::auth_role))
        .route("/system/user/authRole", put(system::user::save_auth_role))
        .route("/system/user/list/dept/{deptId}", get(system::user::list_by_dept))
        .route("/system/user/profile", get(system::profile::get).put(system::profile::update))
        .route("/system/user/profile/updatePwd", put(system::profile::update_pwd))
        .route("/system/user/profile/avatar", post(system::profile::upload_avatar))
        .route("/system/user/resetPwd", put(system::user::reset_pwd))
        .route("/system/user/changeStatus", put(system::user::change_status))
        .route("/system/user", get(system::user::get_empty).post(system::user::add).put(system::user::edit))
        .route("/system/user/{userId}", get(system::user::get_by_id))
        .route("/system/user/{userId}", delete(system::user::remove))
        // ---- system/role ----
        .route("/system/role/list", get(system::role::list))
        .route("/system/role/optionselect", get(system::role::optionselect))
        .route("/system/role/dataScope", put(system::role::data_scope))
        .route("/system/role/changeStatus", put(system::role::change_status))
        .route("/system/role/authUser/allocatedList", get(system::role::allocated_list))
        .route("/system/role/authUser/unallocatedList", get(system::role::unallocated_list))
        .route("/system/role/authUser/cancel", put(system::role::auth_user_cancel))
        .route("/system/role/authUser/cancelAll", put(system::role::auth_user_cancel_all))
        .route("/system/role/authUser/selectAll", put(system::role::auth_user_select_all))
        .route("/system/role/deptTree/{roleId}", get(system::role::dept_tree))
        .route("/system/role/{roleId}", get(system::role::get_by_id))
        .route("/system/role", post(system::role::add).put(system::role::edit))
        .route("/system/role/{roleId}", delete(system::role::remove))
        // ---- system/menu ----
        .route("/system/menu/getRouters", get(system::menu::get_routers))
        .route("/system/menu/list", get(system::menu::list))
        .route("/system/menu/treeselect", get(system::menu::treeselect))
        .route("/system/menu/roleMenuTreeselect/{roleId}", get(system::menu::role_menu_treeselect))
        .route("/system/menu/{menuId}", get(system::menu::get_by_id))
        .route("/system/menu/cascade/{menuIds}", delete(system::menu::cascade_remove))
        .route("/system/menu/{menuId}", delete(system::menu::remove))
        .route("/system/menu", post(system::menu::add).put(system::menu::edit))
        // ---- system/dept ----
        .route("/system/dept/list", get(system::dept::list))
        .route("/system/dept/optionselect", get(system::dept::optionselect))
        .route("/system/dept/list/exclude/{deptId}", get(system::dept::exclude_child))
        .route("/system/dept/{deptId}", get(system::dept::get_by_id))
        .route("/system/dept", post(system::dept::add).put(system::dept::edit))
        .route("/system/dept/{deptId}", delete(system::dept::remove))
        // ---- system/post ----
        .route("/system/post/list", get(system::post::list))
        .route("/system/post/optionselect", get(system::post::optionselect))
        .route("/system/post/deptTree", get(system::post::dept_tree))
        .route("/system/post/{postId}", get(system::post::get_by_id))
        .route("/system/post", post(system::post::add).put(system::post::edit))
        .route("/system/post/{postId}", delete(system::post::remove))
        // ---- system/dict ----
        .route("/system/dict/type/list", get(system::dict::type_list))
        .route("/system/dict/type/optionselect", get(system::dict::type_optionselect))
        .route("/system/dict/type/refreshCache", delete(system::dict::type_refresh_cache))
        .route("/system/dict/type/{dictId}", get(system::dict::type_get_by_id))
        .route("/system/dict/type", post(system::dict::type_add).put(system::dict::type_edit))
        .route("/system/dict/type/{dictId}", delete(system::dict::type_remove))
        .route("/system/dict/data/type/{dictType}", get(system::dict::data_by_type))
        .route("/system/dict/data/list", get(system::dict::data_list))
        .route("/system/dict/data/{dictCode}", get(system::dict::data_get_by_id))
        .route("/system/dict/data", post(system::dict::data_add).put(system::dict::data_edit))
        .route("/system/dict/data/{dictCode}", delete(system::dict::data_remove))
        // ---- system/config ----
        .route("/system/config/list", get(system::config::list))
        .route("/system/config/configKey/{configKey}", get(system::config::get_by_key))
        .route("/system/config/refreshCache", delete(system::config::refresh_cache))
        .route("/system/config/updateByKey", put(system::config::update_by_key))
        .route("/system/config/{configId}", get(system::config::get_by_id))
        .route("/system/config", post(system::config::add).put(system::config::edit))
        .route("/system/config/{configId}", delete(system::config::remove))
        // ---- system/notice ----
        .route("/system/notice/list", get(system::notice::list))
        .route("/system/notice/{noticeId}", get(system::notice::get_by_id))
        .route("/system/notice", post(system::notice::add).put(system::notice::edit))
        .route("/system/notice/{noticeId}", delete(system::notice::remove))
        // ---- monitor ----
        .route("/monitor/online/list", get(monitor::online_list))
        .route("/monitor/online", get(monitor::online_list))
        .route("/monitor/online/myself/{tokenId}", delete(monitor::online_kick_myself))
        .route("/monitor/online/{tokenId}", delete(monitor::online_kick))
        .route("/monitor/logininfor/list", get(monitor::logininfor_list))
        .route("/monitor/logininfor/clean", delete(monitor::logininfor_clean))
        .route("/monitor/logininfor/unlock/{userName}", get(monitor::logininfor_unlock))
        .route("/monitor/logininfor/{infoId}", delete(monitor::logininfor_remove))
        .route("/monitor/operlog/list", get(monitor::operlog_list))
        .route("/monitor/operlog/clean", delete(monitor::operlog_clean))
        .route("/monitor/operlog/{operIds}", delete(monitor::operlog_remove))
}

#[tokio::main]
async fn main() {
    let cfg = config::Config::from_env();
    let conn = match db::open_db(&cfg) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[ruoyi-rs] FATAL: {e}");
            std::process::exit(1);
        }
    };
    let rsa_key = match crypto::load_private_key() {
        Ok(k) => Some(k),
        Err(e) => {
            eprintln!("[ruoyi-rs] WARN: RSA key unavailable, login decryption disabled: {e}");
            None
        }
    };
    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        cfg: cfg.clone(),
        ids: id::Ids::new(),
        rsa_key,
        sessions: Mutex::new(std::collections::HashMap::new()),
        captchas: Mutex::new(std::collections::HashMap::new()),
        pwd_fails: Mutex::new(std::collections::HashMap::new()),
    });

    let api = api_router();
    let api_bare = api.clone().layer(axum::middleware::from_fn(web::spa_document_guard));
    let mut app = Router::new()
        // plus-ui production builds call /prod-api/** (nginx strips the prefix
        // in the Java deployment; here the server handles both spellings).
        .nest("/prod-api", api)
        .merge(api_bare)
        .fallback(web::static_handler);
    // Optional CORS (e.g. GitHub Pages frontend calling this backend directly).
    if !cfg.cors_origin.is_empty() {
        use tower_http::cors::{Any, CorsLayer};
        let cors = if cfg.cors_origin.trim() == "*" {
            CorsLayer::new().allow_origin(Any)
        } else {
            let origins: Vec<axum::http::HeaderValue> = cfg
                .cors_origin
                .split(',')
                .filter_map(|o| o.trim().parse().ok())
                .collect();
            CorsLayer::new().allow_origin(origins)
        };
        let cors = cors
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::PUT, axum::http::Method::DELETE, axum::http::Method::OPTIONS])
            .allow_headers(Any);
        app = app.layer(cors);
    }
    let app = app.with_state(state);

    let addr = format!("0.0.0.0:{}", cfg.port);
    let listener = TcpListener::bind(&addr).await.expect("bind failed");
    eprintln!("[ruoyi-rs] RuoYi-Vue-Plus Rust backend listening on http://{addr}");
    eprintln!("[ruoyi-rs] web dir: {}", cfg.web_dir);
    axum::serve(listener, app).await.expect("server exited");
}
