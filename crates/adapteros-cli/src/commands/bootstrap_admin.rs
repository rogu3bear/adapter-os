use crate::output::OutputWriter;
use adapteros_db::{users::Role, Database};
use anyhow::{anyhow, bail, Context, Result};
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use rand::{distributions::Alphanumeric, rngs::OsRng, thread_rng, Rng};
use serde::Serialize;

#[derive(Serialize)]
struct BootstrapAdminResponse {
    user_id: String,
    email: String,
    display_name: String,
    role: String,
    password: String,
    instructions: &'static str,
}

fn derive_display_name(email: &str) -> String {
    email
        .split('@')
        .next()
        .map(|prefix| {
            if prefix.is_empty() {
                email.to_string()
            } else {
                prefix
                    .split(|c: char| c == '.' || c == '_' || c == '-')
                    .filter(|segment| !segment.is_empty())
                    .map(|segment| {
                        let mut chars = segment.chars();
                        match chars.next() {
                            Some(first) => {
                                let mut title = first.to_uppercase().collect::<String>();
                                title.push_str(&chars.as_str().to_lowercase());
                                title
                            }
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        })
        .unwrap_or_else(|| email.to_string())
}

fn generate_password() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect()
}

pub async fn run(email: &str, display_name: Option<&str>, output: &OutputWriter) -> Result<()> {
    if email.trim().is_empty() {
        bail!("email must not be empty");
    }

    let db = Database::connect_env()
        .await
        .context("failed to connect to database")?;

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(db.pool())
        .await
        .context("failed to count users")?;

    if user_count > 0 {
        bail!("users table already contains entries ({}); bootstrap-admin is intended only for empty deployments", user_count);
    }

    let display_name = display_name
        .filter(|name| !name.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| derive_display_name(email));

    let password = generate_password();

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("failed to hash password: {}", e))?
        .to_string();

    let user_id = db
        .create_user(email, &display_name, &password_hash, Role::Admin)
        .await
        .context("failed to insert admin user")?;

    if output.is_json() {
        let response = BootstrapAdminResponse {
            user_id,
            email: email.to_string(),
            display_name: display_name.clone(),
            role: Role::Admin.to_string(),
            password: password.clone(),
            instructions: "Store the password securely and rotate it after first login.",
        };
        output.json(&response)?;
        return Ok(());
    }

    output.success("Admin account created");
    output.kv("Email", email);
    output.kv("Display name", &display_name);
    output.kv("Role", "admin");
    output.kv("Password", &password);
    output.info("Store the password securely and rotate it immediately after the first login.");

    Ok(())
}
