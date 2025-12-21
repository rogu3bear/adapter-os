use adapteros_core::{AosError, Result};
use adapteros_crypto::envelope::{decrypt_envelope, encrypt_envelope};
use base64::Engine;
use blake3::Hasher;
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use rand::RngCore;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
type HmacSha1 = Hmac<sha1::Sha1>;

const BACKUP_CODE_LENGTH: usize = 10;
const BACKUP_CODE_COUNT: usize = 10;
const TOTP_STEP: u64 = 30;
const TOTP_DIGITS: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCode {
    pub salt_b64: String,
    pub hash_hex: String,
    pub used: bool,
}

pub fn derive_mfa_key(jwt_secret: &[u8]) -> [u8; 32] {
    *blake3::hash(jwt_secret).as_bytes()
}

pub fn encrypt_mfa_secret(secret: &[u8], key: &[u8; 32]) -> Result<String> {
    let (cipher, nonce) = encrypt_envelope(key, secret)?;
    let nonce_b64 = base64::engine::general_purpose::STANDARD.encode(nonce);
    let cipher_b64 = base64::engine::general_purpose::STANDARD.encode(cipher);
    Ok(format!("{}:{}", nonce_b64, cipher_b64))
}

pub fn decrypt_mfa_secret(enc: &str, key: &[u8; 32]) -> Result<Vec<u8>> {
    let parts: Vec<&str> = enc.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(AosError::Parse("invalid mfa secret encoding".into()));
    }
    let nonce_bytes: [u8; 12] = base64::engine::general_purpose::STANDARD
        .decode(parts[0])
        .map_err(|e| AosError::Parse(format!("invalid mfa nonce: {}", e)))?
        .try_into()
        .map_err(|_| AosError::Parse("invalid nonce length".into()))?;
    let cipher_bytes = base64::engine::general_purpose::STANDARD
        .decode(parts[1])
        .map_err(|e| AosError::Parse(format!("invalid mfa ciphertext: {}", e)))?;

    decrypt_envelope(key, &cipher_bytes, &nonce_bytes)
}

pub fn generate_totp_secret() -> (Vec<u8>, String) {
    let mut secret = vec![0u8; 20];
    OsRng.fill_bytes(&mut secret);
    let b32 = BASE32_NOPAD.encode(&secret);
    (secret, b32)
}

pub fn otpauth_uri(email: &str, issuer: &str, b32_secret: &str) -> String {
    // Standard otpauth URI (issuer and label URL-encoded)
    let label_raw = format!("{}:{}", issuer, email);
    let label = urlencoding::encode(&label_raw);
    let issuer_enc = urlencoding::encode(issuer);
    format!(
        "otpauth://totp/{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        label, b32_secret, issuer_enc, TOTP_DIGITS, TOTP_STEP
    )
}

pub fn verify_totp(secret: &[u8], code: &str) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    verify_totp_at(secret, code, now)
}

fn verify_totp_at(secret: &[u8], code: &str, now: u64) -> bool {
    let parsed = match code.trim().parse::<u32>() {
        Ok(v) => v,
        Err(_) => return false,
    };

    // Allow small window (+/- 1 step)
    for offset in [-1i64, 0, 1] {
        let t = if offset.is_negative() {
            now.saturating_sub(TOTP_STEP.saturating_mul(offset.wrapping_abs() as u64))
        } else {
            now.saturating_add(TOTP_STEP.saturating_mul(offset as u64))
        };
        let generated = totp_raw_custom_time(secret, TOTP_DIGITS, TOTP_STEP, t);
        if generated == parsed {
            return true;
        }
    }
    false
}

// Minimal TOTP implementation (HMAC-SHA1 per RFC 6238)
fn totp_raw_custom_time(secret: &[u8], digits: usize, step: u64, timestamp: u64) -> u32 {
    let counter = timestamp / step;
    hotp(secret, counter, digits)
}

fn hotp(secret: &[u8], counter: u64, digits: usize) -> u32 {
    let mut mac = HmacSha1::new_from_slice(secret).expect("HMAC can take key of any size for SHA1");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let offset = (result[19] & 0x0f) as usize;
    let binary: u32 = ((u32::from(result[offset] & 0x7f)) << 24)
        | ((u32::from(result[offset + 1])) << 16)
        | ((u32::from(result[offset + 2])) << 8)
        | (u32::from(result[offset + 3]));

    let modulo = 10u32.pow(digits as u32);
    binary % modulo
}

pub fn generate_backup_codes() -> Vec<String> {
    (0..BACKUP_CODE_COUNT)
        .map(|_| {
            OsRng
                .sample_iter(&Alphanumeric)
                .take(BACKUP_CODE_LENGTH)
                .map(char::from)
                .collect::<String>()
        })
        .collect()
}

pub fn hash_backup_codes(codes: &[String]) -> Vec<BackupCode> {
    codes
        .iter()
        .map(|code| {
            let mut salt = [0u8; 16];
            OsRng.fill_bytes(&mut salt);
            let mut hasher = Hasher::new();
            hasher.update(&salt);
            hasher.update(code.as_bytes());
            BackupCode {
                salt_b64: base64::engine::general_purpose::STANDARD.encode(salt),
                hash_hex: hasher.finalize().to_hex().to_string(),
                used: false,
            }
        })
        .collect()
}

pub fn verify_and_mark_backup_code(codes: &mut [BackupCode], candidate: &str) -> Option<()> {
    for code in codes.iter_mut() {
        if code.used {
            continue;
        }
        let salt = base64::engine::general_purpose::STANDARD
            .decode(&code.salt_b64)
            .ok()?;
        let mut hasher = Hasher::new();
        hasher.update(&salt);
        hasher.update(candidate.as_bytes());
        let computed = hasher.finalize().to_hex().to_string();
        if computed == code.hash_hex {
            code.used = true;
            return Some(());
        }
    }
    None
}
