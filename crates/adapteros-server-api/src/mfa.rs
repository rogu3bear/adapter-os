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

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // TOTP Verification Tests
    // ========================================================================

    #[test]
    fn test_totp_current_window_code_accepted() {
        let secret = b"12345678901234567890"; // 20-byte test secret
        let timestamp = 1_700_000_000u64; // Fixed timestamp

        // Generate code for this exact timestamp
        let expected_code = totp_raw_custom_time(secret, TOTP_DIGITS, TOTP_STEP, timestamp);
        let code_str = format!("{:06}", expected_code);

        assert!(
            verify_totp_at(secret, &code_str, timestamp),
            "Current window code should be accepted"
        );
    }

    #[test]
    fn test_totp_previous_window_code_accepted() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        // Generate code for previous 30-second window
        let prev_timestamp = timestamp - TOTP_STEP;
        let prev_code = totp_raw_custom_time(secret, TOTP_DIGITS, TOTP_STEP, prev_timestamp);
        let code_str = format!("{:06}", prev_code);

        assert!(
            verify_totp_at(secret, &code_str, timestamp),
            "Previous window code should be accepted"
        );
    }

    #[test]
    fn test_totp_next_window_code_accepted() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        // Generate code for next 30-second window
        let next_timestamp = timestamp + TOTP_STEP;
        let next_code = totp_raw_custom_time(secret, TOTP_DIGITS, TOTP_STEP, next_timestamp);
        let code_str = format!("{:06}", next_code);

        assert!(
            verify_totp_at(secret, &code_str, timestamp),
            "Next window code should be accepted"
        );
    }

    #[test]
    fn test_totp_expired_code_rejected() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        // Generate code for 2 windows ago (60+ seconds old)
        let old_timestamp = timestamp - (TOTP_STEP * 2);
        let old_code = totp_raw_custom_time(secret, TOTP_DIGITS, TOTP_STEP, old_timestamp);
        let code_str = format!("{:06}", old_code);

        assert!(
            !verify_totp_at(secret, &code_str, timestamp),
            "Expired code (2 windows ago) should be rejected"
        );
    }

    #[test]
    fn test_totp_future_code_rejected() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        // Generate code for 2 windows in the future
        let future_timestamp = timestamp + (TOTP_STEP * 2);
        let future_code = totp_raw_custom_time(secret, TOTP_DIGITS, TOTP_STEP, future_timestamp);
        let code_str = format!("{:06}", future_code);

        assert!(
            !verify_totp_at(secret, &code_str, timestamp),
            "Future code (2 windows ahead) should be rejected"
        );
    }

    #[test]
    fn test_totp_wrong_code_rejected() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        // Use a random wrong code
        assert!(
            !verify_totp_at(secret, "999999", timestamp),
            "Wrong code should be rejected"
        );
    }

    // ========================================================================
    // TOTP Input Validation Tests
    // ========================================================================

    #[test]
    fn test_totp_non_numeric_code_returns_false() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        assert!(!verify_totp_at(secret, "ABCDEF", timestamp));
        assert!(!verify_totp_at(secret, "12A456", timestamp));
        assert!(!verify_totp_at(secret, "!@#$%^", timestamp));
    }

    #[test]
    fn test_totp_leading_zeros_handled() {
        let secret = b"12345678901234567890";
        // Find a timestamp that produces a code with leading zeros
        // We'll just test that codes with leading zeros parse correctly
        assert!(!verify_totp_at(secret, "000000", 0)); // Likely wrong but parses
        assert!(!verify_totp_at(secret, "000001", 0)); // Parses as 1
    }

    #[test]
    fn test_totp_empty_code_returns_false() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        assert!(!verify_totp_at(secret, "", timestamp));
    }

    #[test]
    fn test_totp_whitespace_trimmed() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        let expected_code = totp_raw_custom_time(secret, TOTP_DIGITS, TOTP_STEP, timestamp);
        let code_with_spaces = format!("  {:06}  ", expected_code);

        assert!(
            verify_totp_at(secret, &code_with_spaces, timestamp),
            "Whitespace should be trimmed"
        );
    }

    #[test]
    fn test_totp_oversized_code_if_numeric() {
        let secret = b"12345678901234567890";
        let timestamp = 1_700_000_000u64;

        // Oversized numeric string - will parse to a large u32
        // and won't match any 6-digit code
        assert!(!verify_totp_at(secret, "123456789", timestamp));
    }

    // ========================================================================
    // Backup Code Tests
    // ========================================================================

    #[test]
    fn test_backup_code_generation_count() {
        let codes = generate_backup_codes();
        assert_eq!(codes.len(), BACKUP_CODE_COUNT);
    }

    #[test]
    fn test_backup_code_generation_length() {
        let codes = generate_backup_codes();
        for code in &codes {
            assert_eq!(code.len(), BACKUP_CODE_LENGTH);
        }
    }

    #[test]
    fn test_backup_code_generation_unique() {
        let codes = generate_backup_codes();
        let mut seen = std::collections::HashSet::new();
        for code in &codes {
            assert!(seen.insert(code), "Backup codes should be unique");
        }
    }

    #[test]
    fn test_backup_code_generation_alphanumeric() {
        let codes = generate_backup_codes();
        for code in &codes {
            assert!(
                code.chars().all(|c| c.is_ascii_alphanumeric()),
                "Backup codes should be alphanumeric"
            );
        }
    }

    #[test]
    fn test_backup_code_single_use() {
        let codes = generate_backup_codes();
        let original_code = codes[0].clone();
        let mut hashed = hash_backup_codes(&codes);

        // First use should succeed
        assert!(
            verify_and_mark_backup_code(&mut hashed, &original_code).is_some(),
            "First use should succeed"
        );

        // Second use should fail
        assert!(
            verify_and_mark_backup_code(&mut hashed, &original_code).is_none(),
            "Second use of same code should fail"
        );
    }

    #[test]
    fn test_backup_code_different_codes_work() {
        let codes = generate_backup_codes();
        let code1 = codes[0].clone();
        let code2 = codes[1].clone();
        let mut hashed = hash_backup_codes(&codes);

        // Use first code
        assert!(verify_and_mark_backup_code(&mut hashed, &code1).is_some());

        // Second different code should still work
        assert!(
            verify_and_mark_backup_code(&mut hashed, &code2).is_some(),
            "Different backup code should work after first is used"
        );
    }

    #[test]
    fn test_backup_code_wrong_code_fails() {
        let codes = generate_backup_codes();
        let mut hashed = hash_backup_codes(&codes);

        assert!(
            verify_and_mark_backup_code(&mut hashed, "WRONGCODE1").is_none(),
            "Wrong backup code should fail"
        );
    }

    #[test]
    fn test_backup_code_used_flag_persists() {
        let codes = generate_backup_codes();
        let original_code = codes[0].clone();
        let mut hashed = hash_backup_codes(&codes);

        // Verify initial state
        assert!(!hashed[0].used);

        // Use the code
        verify_and_mark_backup_code(&mut hashed, &original_code);

        // Verify used flag is set
        assert!(hashed[0].used, "Used flag should be set after verification");
    }

    #[test]
    fn test_hash_backup_codes_produces_unique_salts() {
        let codes = generate_backup_codes();
        let hashed = hash_backup_codes(&codes);

        let mut salts = std::collections::HashSet::new();
        for code in &hashed {
            assert!(
                salts.insert(&code.salt_b64),
                "Each backup code should have a unique salt"
            );
        }
    }

    #[test]
    fn test_hash_backup_codes_produces_unique_hashes() {
        let codes = generate_backup_codes();
        let hashed = hash_backup_codes(&codes);

        let mut hashes = std::collections::HashSet::new();
        for code in &hashed {
            assert!(
                hashes.insert(&code.hash_hex),
                "Each backup code should have a unique hash"
            );
        }
    }

    // ========================================================================
    // Encryption Round-Trip Tests
    // ========================================================================

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = derive_mfa_key(b"test-jwt-secret");
        let original_secret = b"20-byte-test-secret!";

        let encrypted = encrypt_mfa_secret(original_secret, &key).unwrap();
        let decrypted = decrypt_mfa_secret(&encrypted, &key).unwrap();

        assert_eq!(
            decrypted, original_secret,
            "Decrypted secret should match original"
        );
    }

    #[test]
    fn test_decrypt_malformed_nonce_fails() {
        let key = derive_mfa_key(b"test-jwt-secret");

        // Missing colon separator
        let result = decrypt_mfa_secret("invalidformat", &key);
        assert!(result.is_err());

        // Invalid base64 nonce
        let result = decrypt_mfa_secret("!!!invalid:validbase64", &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = derive_mfa_key(b"key-one");
        let key2 = derive_mfa_key(b"key-two");
        let original_secret = b"secret-data";

        let encrypted = encrypt_mfa_secret(original_secret, &key1).unwrap();
        let result = decrypt_mfa_secret(&encrypted, &key2);

        // Wrong key should fail decryption (AES-GCM tag verification)
        assert!(result.is_err(), "Decryption with wrong key should fail");
    }

    #[test]
    fn test_decrypt_corrupted_ciphertext_fails() {
        let key = derive_mfa_key(b"test-jwt-secret");
        let original_secret = b"secret-data";

        let encrypted = encrypt_mfa_secret(original_secret, &key).unwrap();

        // Corrupt the ciphertext part
        let parts: Vec<&str> = encrypted.splitn(2, ':').collect();
        let corrupted = format!("{}:AAAA{}", parts[0], &parts[1][4..]);

        let result = decrypt_mfa_secret(&corrupted, &key);
        assert!(
            result.is_err(),
            "Decryption of corrupted ciphertext should fail"
        );
    }

    // ========================================================================
    // Key Derivation Tests
    // ========================================================================

    #[test]
    fn test_key_derivation_deterministic() {
        let jwt_secret = b"consistent-secret";

        let key1 = derive_mfa_key(jwt_secret);
        let key2 = derive_mfa_key(jwt_secret);

        assert_eq!(key1, key2, "Same input should produce same key");
    }

    #[test]
    fn test_key_derivation_different_inputs_different_outputs() {
        let key1 = derive_mfa_key(b"secret-one");
        let key2 = derive_mfa_key(b"secret-two");

        assert_ne!(key1, key2, "Different inputs should produce different keys");
    }

    #[test]
    fn test_key_derivation_produces_32_bytes() {
        let key = derive_mfa_key(b"any-secret");
        assert_eq!(key.len(), 32, "MFA key should be 32 bytes");
    }

    // ========================================================================
    // OtpAuth URI Tests
    // ========================================================================

    #[test]
    fn test_otpauth_uri_format() {
        let uri = otpauth_uri("user@example.com", "AdapterOS", "JBSWY3DPEHPK3PXP");

        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("secret=JBSWY3DPEHPK3PXP"));
        assert!(uri.contains("issuer=AdapterOS"));
        assert!(uri.contains("algorithm=SHA1"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
    }

    #[test]
    fn test_otpauth_uri_special_characters_encoded() {
        let uri = otpauth_uri("user@example.com", "Test & Co.", "SECRET");

        // & should be URL-encoded
        assert!(uri.contains("%26"), "Ampersand should be URL-encoded");
    }

    // ========================================================================
    // TOTP Secret Generation Tests
    // ========================================================================

    #[test]
    fn test_generate_totp_secret_length() {
        let (secret, b32) = generate_totp_secret();

        assert_eq!(secret.len(), 20, "TOTP secret should be 20 bytes");
        assert!(!b32.is_empty(), "Base32 encoding should not be empty");
    }

    #[test]
    fn test_generate_totp_secret_unique() {
        let (secret1, _) = generate_totp_secret();
        let (secret2, _) = generate_totp_secret();

        assert_ne!(secret1, secret2, "Generated secrets should be unique");
    }

    #[test]
    fn test_generate_totp_secret_base32_valid() {
        let (secret, b32) = generate_totp_secret();

        // Verify base32 decodes back to original secret
        let decoded = BASE32_NOPAD.decode(b32.as_bytes()).unwrap();
        assert_eq!(decoded, secret, "Base32 should decode to original secret");
    }

    // ========================================================================
    // HOTP Algorithm Tests (RFC 4226 Compliance)
    // ========================================================================

    #[test]
    fn test_hotp_rfc4226_test_vectors() {
        // RFC 4226 Appendix D test vectors
        // Secret: "12345678901234567890" (20 bytes)
        let secret = b"12345678901234567890";

        // Counter values and expected HOTP values
        let test_vectors = [
            (0u64, 755224u32),
            (1, 287082),
            (2, 359152),
            (3, 969429),
            (4, 338314),
            (5, 254676),
            (6, 287922),
            (7, 162583),
            (8, 399871),
            (9, 520489),
        ];

        for (counter, expected) in test_vectors {
            let computed = hotp(secret, counter, 6);
            assert_eq!(
                computed, expected,
                "HOTP({}) should be {} but got {}",
                counter, expected, computed
            );
        }
    }
}
