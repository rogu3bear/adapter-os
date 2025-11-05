// No imports needed

/// Build script for database crate
///
/// SQLX validation is completely disabled to avoid compilation issues.
/// All queries will be validated at runtime.
fn main() {
    // SQLX validation disabled - all database queries are stubs
    println!("cargo:warning=SQLX validation disabled - all database queries are stubs");
}
