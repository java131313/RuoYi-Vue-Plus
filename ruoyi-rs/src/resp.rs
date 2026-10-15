use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Mirrors RuoYi's R<T>: {"code": int, "msg": string, "data": T?}
/// Like the Java original, `data` is omitted when absent.
///
/// All responses go out with HTTP 200; the frontend decides by `code`
/// (401 -> relogin), exactly like RuoYi-Vue-Plus does.
#[derive(Serialize)]
pub struct R<T: Serialize = serde_json::Value> {
    pub code: i32,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> R<T> {
    pub fn ok(data: T) -> Self {
        R { code: 200, msg: "操作成功".into(), data: Some(data) }
    }
    pub fn ok_msg(data: T, msg: &str) -> Self {
        R { code: 200, msg: msg.into(), data: Some(data) }
    }
    pub fn fail(msg: &str) -> Self {
        R { code: 500, msg: msg.into(), data: None }
    }
    pub fn fail_code(code: i32, msg: &str) -> Self {
        R { code, msg: msg.into(), data: None }
    }
}

/// Common handler return type: R with a JSON payload.
pub type Rv = R<serde_json::Value>;

pub fn ok(data: serde_json::Value) -> Rv {
    R::ok(data)
}
pub fn ok_msg(data: serde_json::Value, msg: &str) -> Rv {
    R::ok_msg(data, msg)
}
pub fn ok_msg_empty(msg: &str) -> Rv {
    R { code: 200, msg: msg.into(), data: None }
}
pub fn fail(msg: &str) -> Rv {
    R::fail(msg)
}
pub fn unauthorized(msg: &str) -> Rv {
    R::fail_code(401, msg)
}
pub fn forbidden(msg: &str) -> Rv {
    R::fail_code(403, msg)
}
pub fn warn(msg: &str) -> Rv {
    R::fail_code(601, msg)
}

impl<T: Serialize> IntoResponse for R<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// Mirrors RuoYi's TableDataInfo: {"total": n, "rows": [...], "code": 200, "msg": "查询成功"}
#[derive(Serialize)]
pub struct TableDataInfo {
    pub total: i64,
    pub rows: serde_json::Value,
    pub code: i32,
    pub msg: String,
}

impl TableDataInfo {
    pub fn new(total: i64, rows: serde_json::Value) -> Self {
        TableDataInfo { total, rows, code: 200, msg: "查询成功".into() }
    }
}

impl IntoResponse for TableDataInfo {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// Unified handler return: either an R envelope or a top-level TableDataInfo
/// (RuoYi list endpoints return TableDataInfo directly, NOT wrapped in R).
pub enum Resp {
    Json(Rv),
    Table(TableDataInfo),
}

impl From<Rv> for Resp {
    fn from(r: Rv) -> Self {
        Resp::Json(r)
    }
}

impl From<TableDataInfo> for Resp {
    fn from(t: TableDataInfo) -> Self {
        Resp::Table(t)
    }
}

impl IntoResponse for Resp {
    fn into_response(self) -> Response {
        match self {
            Resp::Json(r) => r.into_response(),
            Resp::Table(t) => t.into_response(),
        }
    }
}
