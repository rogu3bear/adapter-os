//! Initialize tenant

use crate::output::OutputWriter;
use adapteros_registry::Registry;
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
struct TenantInfo {
    id: String,
    uid: u32,
    gid: u32,
    directory: String,
}

pub async fn run(id: &str, uid: u32, gid: u32, output: &OutputWriter) -> Result<()> {
    output.info(format!("Initializing tenant: {}", id));

    let registry = Registry::open("registry.db")?;
    registry.register_tenant(id, uid, gid)?;

    output.success(format!(
        "Tenant {} registered with UID {} and GID {}",
        id, uid, gid
    ));

    // Create tenant directories
    let tenant_root = format!("/var/run/aos/{}", id);
    std::fs::create_dir_all(&tenant_root)?;

    output.success(format!("Created tenant directory: {}", tenant_root));

    if output.is_json() {
        let info = TenantInfo {
            id: id.to_string(),
            uid,
            gid,
            directory: tenant_root,
        };
        output.json(&info)?;
    }

    Ok(())
}
