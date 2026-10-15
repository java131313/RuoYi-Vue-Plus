use std::env;

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    /// SQLite file path
    pub db_path: String,
    /// Directory holding the built frontend (plus-ui dist)
    pub web_dir: String,
    /// Directory for uploaded files (served at /uploads/)
    pub upload_dir: String,
    /// JWT HMAC secret
    pub jwt_secret: String,
    /// captcha on/off (matches `captcha.enable` in application.yml)
    pub captcha_enabled: bool,
    /// login fail lock: max retries and lock minutes
    pub pwd_max_retry: u32,
    pub pwd_lock_minutes: u64,
    /// Allowed CORS origin(s) for cross-site frontends (e.g. GitHub Pages).
    /// Empty = same-origin only. "*" = any origin. Comma list = those origins.
    pub cors_origin: String,
}

impl Config {
    pub fn from_env() -> Self {
        let port = env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(8080);
        let db_path = env::var("DB_PATH").unwrap_or_else(|_| "data/ruoyi.db".into());
        let web_dir = env::var("WEB_DIR").unwrap_or_else(|_| "web/dist".into());
        let upload_dir = env::var("UPLOAD_DIR").unwrap_or_else(|_| "web/uploads".into());
        let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
            // Same default as sa-token.jwt-secret-key in application.yml so a
            // stock plus-ui build works out of the box. Override in production!
            "71EAE522D34B47E6A039B2265A1ED345C7A347A9D14A48B51B7AFBB75BCEB304".into()
        });
        let captcha_enabled = env::var("CAPTCHA_ENABLED").map(|v| v != "false").unwrap_or(true);
        let cors_origin = env::var("CORS_ORIGIN").unwrap_or_default();
        Self {
            port,
            db_path,
            web_dir,
            upload_dir,
            jwt_secret,
            captcha_enabled,
            pwd_max_retry: 5,
            pwd_lock_minutes: 10,
            cors_origin,
        }
    }
}
