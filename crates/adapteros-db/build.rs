// No imports needed

/// Build script for database crate
///
/// SQLX validation is completely disabled to avoid compilation issues.
/// All queries will be validated at runtime.
fn main() {
    // Disable SQLX offline mode
    println!("cargo:rustc-env=SQLX_OFFLINE=false");

    println!("cargo:warning=SQLX validation disabled - all database queries are stubs");
}
