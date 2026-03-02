//! Error conversion macros for reducing boilerplate in `From` implementations.
//!
//! This module provides macros to generate `From` implementations for error types,
//! reducing the repetitive pattern of converting errors to `AosError` variants.
//!
//! # Usage
//!
//! ## Basic conversion (uses `.to_string()` on the source error)
//!
//! ```rust,ignore
//! impl_error_from!(std::io::Error => Io);
//! impl_error_from!(rusqlite::Error => Sqlite);
//! ```
//!
//! This generates:
//! ```rust,ignore
//! impl From<std::io::Error> for AosError {
//!     fn from(err: std::io::Error) -> Self {
//!         AosError::Io(err.to_string())
//!     }
//! }
//! ```
//!
//! ## With error chain preservation
//!
//! For errors that implement `std::error::Error`, you can preserve the full
//! error chain (including all `.source()` causes):
//!
//! ```rust,ignore
//! impl_error_from!(std::io::Error => Io, chain);
//! ```
//!
//! This generates an implementation that walks the error chain and joins all
//! messages with " -> ", producing output like:
//! ```text
//! "Connection refused -> DNS lookup failed -> No such host"
//! ```
//!
//! ## With custom transformation
//!
//! ```rust,ignore
//! impl_error_from!(anyhow::Error => Internal, |e| format!("Internal: {}", e));
//! ```
//!
//! This generates:
//! ```rust,ignore
//! impl From<anyhow::Error> for AosError {
//!     fn from(err: anyhow::Error) -> Self {
//!         AosError::Internal((|e| format!("Internal: {}", e))(err))
//!     }
//! }
//! ```
//!
//! ## With prefix (adds a context prefix to the message)
//!
//! ```rust,ignore
//! impl_error_from!(zip::result::ZipError => Io, prefix = "Zip operation failed");
//! ```
//!
//! This generates:
//! ```rust,ignore
//! impl From<zip::result::ZipError> for AosError {
//!     fn from(err: zip::result::ZipError) -> Self {
//!         AosError::Io(format!("Zip operation failed: {}", err))
//!     }
//! }
//! ```
//!
//! ## Batch conversion for multiple types
//!
//! ```rust,ignore
//! impl_error_from_batch!(AosError {
//!     std::io::Error => Io,
//!     rusqlite::Error => Sqlite,
//!     serde_json::Error => Internal, prefix = "JSON error",
//! });
//! ```
//!
//! # Design Principles
//!
//! 1. **Minimal overhead**: The macros expand to simple, efficient code with no runtime cost.
//! 2. **Explicit mapping**: Each conversion is explicitly declared, making error handling visible.
//! 3. **Flexible transforms**: Support for custom transformation closures when needed.
//! 4. **Consistent formatting**: Error messages follow the project's error message standards.
//! 5. **Chain preservation**: Optional error chain preservation for debugging context.

/// Format an error chain into a single string.
///
/// Walks the error's source chain and joins all messages with " -> ".
/// This preserves context that would otherwise be lost during error conversion.
///
/// # Example output
/// ```text
/// "Connection refused -> DNS lookup failed -> No such host"
/// ```
#[doc(hidden)]
pub fn format_error_chain(err: &dyn std::error::Error) -> String {
    let mut chain = vec![err.to_string()];
    let mut current = err.source();
    while let Some(cause) = current {
        chain.push(cause.to_string());
        current = cause.source();
    }
    chain.join(" -> ")
}

/// Generate a `From` implementation for converting a source error type to `AosError`.
///
/// # Variants
///
/// ## Basic: `impl_error_from!(SourceType => Variant)`
///
/// Converts using `.to_string()` on the source error.
///
/// ## With chain: `impl_error_from!(SourceType => Variant, chain)`
///
/// Preserves the full error chain by walking `.source()` and joining with " -> ".
/// Use this when the source error may have nested causes you want to preserve.
///
/// ## With transform: `impl_error_from!(SourceType => Variant, |e| transform(e))`
///
/// Applies a custom transformation closure to the source error.
///
/// ## With prefix: `impl_error_from!(SourceType => Variant, prefix = "context")`
///
/// Adds a prefix to the error message: `"context: {err}"`.
///
/// ## With prefix and chain: `impl_error_from!(SourceType => Variant, prefix = "context", chain)`
///
/// Combines prefix with error chain preservation.
///
/// # Examples
///
/// ```rust,ignore
/// use adapteros_core::{impl_error_from, AosError};
///
/// // Basic conversion
/// impl_error_from!(std::io::Error => Io);
///
/// // With error chain preservation
/// impl_error_from!(std::io::Error => Io, chain);
///
/// // With custom transform
/// impl_error_from!(MyError => Internal, |e| format!("my context: {}", e));
///
/// // With prefix
/// impl_error_from!(ConfigError => Config, prefix = "Configuration failed");
///
/// // With prefix and chain
/// impl_error_from!(ZipError => Io, prefix = "Zip operation failed", chain);
/// ```
#[macro_export]
macro_rules! impl_error_from {
    // Basic: source type => variant (uses .to_string())
    ($source:ty => $variant:ident) => {
        impl From<$source> for $crate::AosError {
            fn from(err: $source) -> Self {
                $crate::AosError::$variant(err.to_string())
            }
        }
    };

    // With error chain preservation (walks .source() chain)
    ($source:ty => $variant:ident, chain) => {
        impl From<$source> for $crate::AosError {
            fn from(err: $source) -> Self {
                $crate::AosError::$variant($crate::error_macros::format_error_chain(&err))
            }
        }
    };

    // With prefix (adds context prefix to message) - must come before $transform:expr
    ($source:ty => $variant:ident, prefix = $prefix:literal) => {
        impl From<$source> for $crate::AosError {
            fn from(err: $source) -> Self {
                $crate::AosError::$variant(format!("{}: {}", $prefix, err))
            }
        }
    };

    // With custom transform closure - must come after more specific patterns
    ($source:ty => $variant:ident, $transform:expr) => {
        impl From<$source> for $crate::AosError {
            fn from(err: $source) -> Self {
                $crate::AosError::$variant($transform(err))
            }
        }
    };

    // With prefix and chain (combines prefix with error chain preservation)
    ($source:ty => $variant:ident, prefix = $prefix:literal, chain) => {
        impl From<$source> for $crate::AosError {
            fn from(err: $source) -> Self {
                $crate::AosError::$variant(format!(
                    "{}: {}",
                    $prefix,
                    $crate::error_macros::format_error_chain(&err)
                ))
            }
        }
    };
}

/// Generate multiple `From` implementations in a single macro invocation.
///
/// This is useful for converting many error types at once.
///
/// # Examples
///
/// ```rust,ignore
/// use adapteros_core::{impl_error_from_batch, AosError};
///
/// impl_error_from_batch!(AosError {
///     std::io::Error => Io,
///     rusqlite::Error => Sqlite,
///     anyhow::Error => Internal,
/// });
/// ```
///
/// With mixed variants:
///
/// ```rust,ignore
/// impl_error_from_batch!(AosError {
///     std::io::Error => Io,
///     MyError => Internal, prefix = "My context",
///     OtherError => Worker, |e| format!("worker: {}", e),
/// });
/// ```
#[macro_export]
macro_rules! impl_error_from_batch {
    // Match the error type name and a list of conversions
    ($error_type:ty { $($source:ty => $variant:ident $(, $($rest:tt)+)?),* $(,)? }) => {
        $(
            $crate::impl_error_from!($source => $variant $(, $($rest)+)?);
        )*
    };
}

/// Generate a `From` implementation for a custom error type (not just AosError).
///
/// This is useful when you have crate-local error types that need similar conversions.
///
/// # Examples
///
/// ```rust,ignore
/// use adapteros_core::impl_error_from_for;
///
/// pub enum MyLocalError {
///     Io(String),
///     Parse(String),
/// }
///
/// impl_error_from_for!(MyLocalError: std::io::Error => Io);
/// impl_error_from_for!(MyLocalError: std::num::ParseIntError => Parse);
/// ```
#[macro_export]
macro_rules! impl_error_from_for {
    // Basic: target: source type => variant
    ($target:ty: $source:ty => $variant:ident) => {
        impl From<$source> for $target {
            fn from(err: $source) -> Self {
                <$target>::$variant(err.to_string())
            }
        }
    };

    // With error chain preservation
    ($target:ty: $source:ty => $variant:ident, chain) => {
        impl From<$source> for $target {
            fn from(err: $source) -> Self {
                <$target>::$variant($crate::error_macros::format_error_chain(&err))
            }
        }
    };

    // With prefix
    ($target:ty: $source:ty => $variant:ident, prefix = $prefix:literal) => {
        impl From<$source> for $target {
            fn from(err: $source) -> Self {
                <$target>::$variant(format!("{}: {}", $prefix, err))
            }
        }
    };

    // With prefix and chain
    ($target:ty: $source:ty => $variant:ident, prefix = $prefix:literal, chain) => {
        impl From<$source> for $target {
            fn from(err: $source) -> Self {
                <$target>::$variant(format!(
                    "{}: {}",
                    $prefix,
                    $crate::error_macros::format_error_chain(&err)
                ))
            }
        }
    };

    // With custom transform - must come after prefix variants
    ($target:ty: $source:ty => $variant:ident, $transform:expr) => {
        impl From<$source> for $target {
            fn from(err: $source) -> Self {
                <$target>::$variant($transform(err))
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::AosError;

    // Test custom error type for macro testing
    #[derive(Debug)]
    struct TestSourceError(String);

    impl std::fmt::Display for TestSourceError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for TestSourceError {}

    // Test basic conversion
    impl_error_from!(TestSourceError => Internal);

    #[test]
    fn test_basic_conversion() {
        let source = TestSourceError("test error".to_string());
        let aos_err: AosError = source.into();
        match aos_err {
            AosError::Internal(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected Internal variant"),
        }
    }

    // Test custom error type for transform testing
    #[derive(Debug)]
    struct AnotherError(i32);

    impl std::fmt::Display for AnotherError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "error code {}", self.0)
        }
    }

    impl std::error::Error for AnotherError {}

    impl_error_from!(AnotherError => Worker, |e: AnotherError| format!("Worker failed with code {}", e.0));

    #[test]
    fn test_transform_conversion() {
        let source = AnotherError(42);
        let aos_err: AosError = source.into();
        match aos_err {
            AosError::Worker(msg) => assert_eq!(msg, "Worker failed with code 42"),
            _ => panic!("Expected Worker variant"),
        }
    }

    // Test custom error type for prefix testing
    #[derive(Debug)]
    struct PrefixError(String);

    impl std::fmt::Display for PrefixError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for PrefixError {}

    impl_error_from!(PrefixError => Config, prefix = "Configuration error");

    #[test]
    fn test_prefix_conversion() {
        let source = PrefixError("invalid port".to_string());
        let aos_err: AosError = source.into();
        match aos_err {
            AosError::Config(msg) => assert_eq!(msg, "Configuration error: invalid port"),
            _ => panic!("Expected Config variant"),
        }
    }

    // Test impl_error_from_for with a local error type
    #[derive(Debug, PartialEq)]
    enum LocalError {
        Parse(String),
        // Network(String),
    }

    impl_error_from_for!(LocalError: std::num::ParseIntError => Parse);

    #[test]
    fn test_impl_error_from_for() {
        let source: Result<i32, _> = "not a number".parse();
        let err = source.unwrap_err();
        let local_err: LocalError = err.into();
        match local_err {
            LocalError::Parse(msg) => assert!(msg.contains("invalid")),
        }
    }

    #[derive(Debug, PartialEq)]
    enum LocalPrefixError {
        Config(String),
    }

    impl_error_from_for!(LocalPrefixError: PrefixError => Config, prefix = "Local config error");

    #[test]
    fn test_impl_error_from_for_prefix() {
        let source = PrefixError("invalid path".to_string());
        let local_err: LocalPrefixError = source.into();
        match local_err {
            LocalPrefixError::Config(msg) => assert_eq!(msg, "Local config error: invalid path"),
        }
    }

    // Test error chain preservation
    #[derive(Debug)]
    struct OuterError {
        message: String,
        source: Option<InnerError>,
    }

    #[derive(Debug)]
    struct InnerError(String);

    impl std::fmt::Display for OuterError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for OuterError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source
                .as_ref()
                .map(|e| e as &(dyn std::error::Error + 'static))
        }
    }

    impl std::fmt::Display for InnerError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for InnerError {}

    impl_error_from!(OuterError => Telemetry, chain);

    #[test]
    fn test_chain_conversion() {
        let inner = InnerError("inner cause".to_string());
        let outer = OuterError {
            message: "outer error".to_string(),
            source: Some(inner),
        };
        let aos_err: AosError = outer.into();
        match aos_err {
            AosError::Telemetry(msg) => {
                assert!(msg.contains("outer error"), "Expected outer error message");
                assert!(msg.contains("inner cause"), "Expected inner cause message");
                assert!(msg.contains(" -> "), "Expected chain separator");
            }
            _ => panic!("Expected Telemetry variant"),
        }
    }

    // Test prefix with chain
    #[derive(Debug)]
    struct ChainPrefixError(String);

    impl std::fmt::Display for ChainPrefixError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for ChainPrefixError {}

    impl_error_from!(ChainPrefixError => Mlx, prefix = "MLX operation failed", chain);

    #[test]
    fn test_prefix_chain_conversion() {
        let source = ChainPrefixError("compute error".to_string());
        let aos_err: AosError = source.into();
        match aos_err {
            AosError::Mlx(msg) => {
                assert!(msg.starts_with("MLX operation failed:"), "Expected prefix");
                assert!(msg.contains("compute error"), "Expected error message");
            }
            _ => panic!("Expected Mlx variant"),
        }
    }
}
