use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[serde(rename = "admin")]
    Admin,
    #[serde(rename = "operator")]
    Operator,
    #[serde(rename = "sre")]
    Sre,
    #[serde(rename = "compliance")]
    Compliance,
    #[serde(rename = "auditor")]
    Auditor,
    #[serde(rename = "viewer")]
    Viewer,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Admin => write!(f, "admin"),
            Role::Operator => write!(f, "operator"),
            Role::Sre => write!(f, "sre"),
            Role::Compliance => write!(f, "compliance"),
            Role::Auditor => write!(f, "auditor"),
            Role::Viewer => write!(f, "viewer"),
        }
    }
}

impl std::str::FromStr for Role {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "admin" => Ok(Role::Admin),
            "operator" => Ok(Role::Operator),
            "sre" => Ok(Role::Sre),
            "compliance" => Ok(Role::Compliance),
            "auditor" => Ok(Role::Auditor),
            "viewer" => Ok(Role::Viewer),
            _ => Err(anyhow::anyhow!("invalid role: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    #[serde(skip_serializing)]
    pub pw_hash: String,
    pub role: String,
    pub disabled: bool,
    pub created_at: String,
}

impl Db {
    pub async fn create_user(
        &self,
        email: &str,
        display_name: &str,
        pw_hash: &str,
        role: Role,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let role_str = role.to_string();
        sqlx::query(
            "INSERT INTO users (id, email, display_name, pw_hash, role) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(email)
        .bind(display_name)
        .bind(pw_hash)
        .bind(&role_str)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT id, email, display_name, pw_hash, role, disabled, created_at FROM users WHERE email = ?"
        )
        .bind(email)
        .fetch_optional(self.pool())
        .await?;
        Ok(user)
    }

    pub async fn get_user(&self, id: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            "SELECT id, email, display_name, pw_hash, role, disabled, created_at FROM users WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(user)
    }
}
