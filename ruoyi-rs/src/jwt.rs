use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Claims {
    /// session token id (used for kick/online lists)
    pub tid: String,
    pub uid: i64,
    pub uname: String,
    pub cid: String,
    pub exp: u64,
    pub iat: u64,
}

pub fn sign(secret: &str, tid: &str, uid: i64, uname: &str, cid: &str, ttl_secs: u64) -> String {
    let now = jsonwebtoken::get_current_timestamp();
    let claims = Claims {
        tid: tid.into(),
        uid,
        uname: uname.into(),
        cid: cid.into(),
        iat: now,
        exp: now + ttl_secs,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .expect("jwt sign failed")
}

pub fn verify(secret: &str, token: &str) -> Option<Claims> {
    let mut val = Validation::default();
    val.validate_exp = true;
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &val)
        .ok()
        .map(|d| d.claims)
}
