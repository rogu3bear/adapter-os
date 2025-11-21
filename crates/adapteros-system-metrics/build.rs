// Build script for system metrics crate
//
// SQLX validation is disabled - all queries use runtime validation via sqlx::query()
fn main() {
    println!("cargo:warning=SQLX validation disabled - all database queries use runtime validation");
}
