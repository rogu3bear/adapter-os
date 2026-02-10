/// Integrity verification strictness for adapter manifests.
///
/// Controls whether a missing `integrity_hash` is accepted or rejected
/// during `verify_integrity()`.
///
/// - `Strict`: Missing hashes are rejected. Use in production / strict determinism.
/// - `Permissive`: Missing hashes are accepted (backward compatibility with legacy
///   `.aos` files). Hashes that _are_ present are still verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegrityMode {
    /// Reject missing integrity hashes. Verify present hashes.
    Strict,
    /// Accept missing integrity hashes. Verify present hashes.
    Permissive,
}

impl IntegrityMode {
    /// Whether missing integrity hashes should cause an error.
    pub fn is_strict(self) -> bool {
        matches!(self, Self::Strict)
    }
}

impl Default for IntegrityMode {
    /// Defaults to `Permissive` for backward compatibility.
    fn default() -> Self {
        Self::Permissive
    }
}

#[cfg(test)]
mod tests {
    use super::IntegrityMode;

    #[test]
    fn strict_is_strict() {
        assert!(IntegrityMode::Strict.is_strict());
        assert!(!IntegrityMode::Permissive.is_strict());
    }

    #[test]
    fn default_is_permissive() {
        assert_eq!(IntegrityMode::default(), IntegrityMode::Permissive);
    }
}
