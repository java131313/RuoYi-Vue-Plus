use std::time::Instant;

/// In-memory login session (replaces Redis Sa-Token storage).
#[allow(dead_code)] // full session record kept for the online-user monitor
pub struct Session {
    pub token_id: String,
    pub user_id: i64,
    pub user_name: String,
    pub dept_id: i64,
    pub dept_name: String,
    pub client_id: String,
    pub ip: String,
    pub browser: String,
    pub os: String,
    pub login_time: String,
}

/// In-memory captcha store entry (replaces Redis `global:captcha_codes:{uuid}`).
pub struct Captcha {
    pub answer: String,
    pub expires: Instant,
}

/// Login failure counter per username (replaces Redis `pwd_err_cnt:{username}`).
pub struct PwdFail {
    pub count: u32,
    pub locked_until: Option<Instant>,
}

impl PwdFail {
    pub fn is_locked(&self) -> bool {
        self.locked_until.map(|t| Instant::now() < t).unwrap_or(false)
    }
}
