//! Decryption for the `@ApiEncrypt` login/register endpoints.
//!
//! plus-ui sends: `encrypt-key` header = base64(RSA-1024/PKCS1v15( base64(aesKey) )),
//! body = base64(AES-256-ECB/PKCS7( JSON )). This mirrors hutool's
//! `decryptByRsa` + `SecureUtil.aes(key).decryptStr` on the Java side.
//! Keys are the fixed demo pair from application.yml (`api-decrypt`).

use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes128;
use aes::Aes192;
use aes::Aes256;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rsa::pkcs8::DecodePrivateKey;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};

/// api-decrypt.privateKey from ruoyi-admin/application.yml (PKCS#8, base64 DER)
pub const PRIVATE_KEY_B64: &str = "MIIBVAIBADANBgkqhkiG9w0BAQEFAASCAT4wggE6AgEAAkEAqhHyZfSsYourNxaY7Nt+PrgrxkiA50efORdI5U5lsW79MmFnusUA355oaSXcLhu5xxB38SMSyP2KvuKNPuH3owIDAQABAkAfoiLyL+Z4lf4Myxk6xUDgLaWGximj20CUf+5BKKnlrK+Ed8gAkM0HqoTt2UZwA5E2MzS4EI2gjfQhz5X28uqxAiEA3wNFxfrCZlSZHb0gn2zDpWowcSxQAgiCstxGUoOqlW8CIQDDOerGKH5OmCJ4Z21v+F25WaHYPxCFMvwxpcw99EcvDQIgIdhDTIqD2jfYjPTY8Jj3EDGPbH2HHuffvflECt3Ek60CIQCFRlCkHpi7hthhYhovyloRYsM+IS9h/0BzlEAuO0ktMQIgSPT3aFAgJYwKpqRYKlLDVcflZFCKY7u3UP8iWi1Qw0Y=";

/// api-decrypt public key (X.509 SPKI, base64 DER) — used only by tests to
/// simulate what the browser's jsencrypt does with VITE_APP_RSA_PUBLIC_KEY.
#[cfg(test)]
pub const PUBLIC_KEY_B64: &str = "MFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAKoR8mX0rGKLqzcWmOzbfj64K8ZIgOdHnzkXSOVOZbFu/TJhZ7rFAN+eaGkl3C4buccQd/EjEsj9ir7ijT7h96MCAwEAAQ==";

pub fn load_private_key() -> Result<RsaPrivateKey, String> {
    let der = B64.decode(PRIVATE_KEY_B64).map_err(|e| format!("private key b64: {e}"))?;
    RsaPrivateKey::from_pkcs8_der(&der).map_err(|e| format!("private key pkcs8: {e}"))
}

/// Lenient base64 decode replicating java.util.Base64.getMimeDecoder (which
/// hutool uses): non-alphabet characters — e.g. the `"` quotes axios adds
/// when it JSON-serializes the encrypted string body — are ignored.
fn b64_lenient(s: &str) -> Result<Vec<u8>, String> {
    let filtered: Vec<u8> = s
        .bytes()
        .filter(|b| b.is_ascii_alphanumeric() || matches!(*b, b'+' | b'/' | b'='))
        .collect();
    B64.decode(filtered).map_err(|e| format!("b64: {e}"))
}

/// Full login-body decryption: RSA-decrypt the header to get the base64 AES
/// key, decode it, then AES-ECB-decrypt the body.
/// Mirrors DecryptRequestBodyWrapper: decryptByRsa -> decryptByBase64 -> decryptByAes.
pub fn decrypt_body(key: &RsaPrivateKey, encrypt_key_header: &str, body: &str) -> Result<String, String> {
    let cipher = b64_lenient(encrypt_key_header.trim()).map_err(|e| format!("header {e}"))?;
    let aes_key_b64 = key.decrypt(Pkcs1v15Encrypt, &cipher).map_err(|e| format!("rsa decrypt: {e}"))?;
    let aes_key_str = String::from_utf8(aes_key_b64).map_err(|e| format!("aes key utf8: {e}"))?;
    let aes_key = b64_lenient(aes_key_str.trim()).map_err(|e| format!("aes key {e}"))?;
    let data = b64_lenient(body.trim()).map_err(|e| format!("body {e}"))?;
    let plain = match aes_key.len() {
        16 => ecb_decrypt::<Aes128>(&aes_key, &data),
        24 => ecb_decrypt::<Aes192>(&aes_key, &data),
        32 => ecb_decrypt::<Aes256>(&aes_key, &data),
        n => Err(format!("bad aes key len {n}")),
    }?;
    String::from_utf8(plain).map_err(|e| format!("utf8: {e}"))
}

fn ecb_decrypt<C>(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String>
where
    C: BlockDecrypt + KeyInit,
{
    if data.is_empty() || data.len() % 16 != 0 {
        return Err("ciphertext not a multiple of 16".into());
    }
    let cipher = C::new_from_slice(key).map_err(|e| format!("aes key: {e}"))?;
    let mut buf = data.to_vec();
    for block in buf.chunks_mut(16) {
        cipher.decrypt_block(aes::cipher::generic_array::GenericArray::from_mut_slice(block));
    }
    pkcs7_unpad(&mut buf)
}

fn pkcs7_unpad(buf: &mut Vec<u8>) -> Result<Vec<u8>, String> {
    let Some(&n) = buf.last() else { return Err("empty".into()) };
    let n = n as usize;
    if n == 0 || n > 16 || n > buf.len() {
        return Err("bad pkcs7 padding".into());
    }
    if buf[buf.len() - n..].iter().any(|&b| b != n as u8) {
        return Err("bad pkcs7 padding bytes".into());
    }
    buf.truncate(buf.len() - n);
    Ok(std::mem::take(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockEncrypt;
    use rsa::pkcs8::DecodePublicKey;
    use rsa::RsaPublicKey;

    #[test]
    fn roundtrip_like_plus_ui() {
        let priv_key = load_private_key().unwrap();
        let pub_der = B64.decode(PUBLIC_KEY_B64).unwrap();
        let pub_key = RsaPublicKey::from_public_key_der(&pub_der).unwrap();

        // browser side: random 32-char AES key (hex), AES-256-ECB/PKCS7
        let aes_key_str = "0123456789abcdef0123456789abcdef";
        let aes_key = aes_key_str.as_bytes();
        let body_json = r#"{"clientId":"e5cd7e4891bf95d1d19206ce24a7b32e","grantType":"password","tenantId":"000000","username":"admin","password":"admin123","code":"5","uuid":"abc"}"#;

        let cipher = Aes256::new_from_slice(aes_key).unwrap();
        let mut padded = body_json.as_bytes().to_vec();
        let pad = 16 - padded.len() % 16;
        padded.extend(std::iter::repeat(pad as u8).take(pad));
        for block in padded.chunks_mut(16) {
            cipher.encrypt_block(aes::cipher::generic_array::GenericArray::from_mut_slice(block));
        }
        let body_b64 = B64.encode(&padded);

        // jsencrypt: RSA/PKCS1v15 of base64(aesKey)
        let enc_key = pub_key
            .encrypt(&mut rand::rngs::OsRng, Pkcs1v15Encrypt, B64.encode(aes_key).as_bytes())
            .unwrap();
        let header_b64 = B64.encode(&enc_key);

        let plain = decrypt_body(&priv_key, &header_b64, &body_b64).unwrap();
        assert_eq!(plain, body_json);

        // axios JSON-serializes the encrypted string body (quoted base64);
        // hutool's MIME base64 decoder ignores the quotes, so must we.
        let quoted = format!("\"{body_b64}\"");
        let plain2 = decrypt_body(&priv_key, &header_b64, &quoted).unwrap();
        assert_eq!(plain2, body_json);
    }

    #[test]
    fn bcrypt_seed_hash_verifies() {
        // admin / admin123 hash from ry_vue_5.X.sql
        assert!(bcrypt::verify("admin123", "$2a$10$7JB720yubVSZvUI0rEqK/.VqGOZTH.ulu33dHOiBE8ByOhJIrdAu2").unwrap());
        assert!(bcrypt::verify("666666", "$2a$10$b8yUzN0C71sbz.PhNOCgJe.Tu1yWC3RNrTyjSQ8p1W0.aaUXUJ.Ne").unwrap());
    }
}
