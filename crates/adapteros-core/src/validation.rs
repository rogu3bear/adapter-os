pub mod validation {
    use crate::AosError;

    pub fn validate_adapter_id(id: &str) -> Result<(), AosError> {
        if id.is_empty() {
            return Err(AosError::Validation("Adapter ID cannot be empty".to_string()));
        }

        if id.len() > 64 {
            return Err(AosError::Validation("Adapter ID must be 64 characters or less".to_string()));
        }

        if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(AosError::Validation(
                "Adapter ID must contain only alphanumeric characters, hyphens, and underscores"
                    .to_string(),
            ));
        }

        Ok(())
    }

    pub fn validate_name(name: &str) -> Result<(), AosError> {
        if name.is_empty() {
            return Err(AosError::Validation("Name cannot be empty".to_string()));
        }

        if name.len() > 128 {
            return Err(AosError::Validation("Name must be 128 characters or less".to_string()));
        }

        if !name.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_') {
            return Err(AosError::Validation(
                "Name must contain only alphanumeric characters, spaces, hyphens, and underscores"
                    .to_string(),
            ));
        }

        Ok(())
    }

    pub fn validate_hash_b3(hash: &str) -> Result<(), AosError> {
        if !hash.starts_with("b3:") {
            return Err(AosError::Validation("Hash must start with 'b3:'".to_string()));
        }

        let hex_part = &hash[3..];
        if hex_part.len() != 64 {
            return Err(AosError::Validation("B3 hash hex part must be 64 characters".to_string()));
        }

        hex::decode(hex_part).map_err(|e| AosError::Validation(format!("Invalid hex in hash: {}", e)))?;

        Ok(())
    }
}
