use crate::db::Shared;
use axum::http::HeaderMap;

/// Endpoints marked `isEncrypt: 'true'` by plus-ui arrive as RSA+AES
/// ciphertext when the `encrypt-key` header is present; otherwise the body
/// is plain JSON. Decode accordingly.
pub async fn maybe_decrypt(st: &Shared, headers: &HeaderMap, body: String) -> String {
    if let Some(enc) = headers.get("encrypt-key").and_then(|v| v.to_str().ok()) {
        if let Some(key) = &st.rsa_key {
            if let Ok(plain) = crate::crypto::decrypt_body(key, enc, &body) {
                return plain;
            }
        }
    }
    body
}
