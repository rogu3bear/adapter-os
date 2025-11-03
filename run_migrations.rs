use adapteros_db::{Database};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::connect("var/cp.db").await?;
    db.migrate().await?;
    println!("Migrations completed successfully");
    Ok(())
}
