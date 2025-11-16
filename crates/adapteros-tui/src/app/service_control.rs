use anyhow::{anyhow, Result};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::process::Command;
use tracing::{debug, info, warn};
use which::which;

pub struct ServiceControl {
    project_root: PathBuf,
    service_manager: PathBuf,
    launch_script: PathBuf,
}

pub struct ServiceCommandResult {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
}

impl ServiceCommandResult {
    pub fn combined_output(&self) -> String {
        let mut combined = String::new();
        if !self.stdout.trim().is_empty() {
            combined.push_str(self.stdout.trim());
        }

        if !self.stderr.trim().is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(self.stderr.trim());
        }

        combined
    }
}

impl ServiceControl {
    pub fn new() -> Result<Self> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let project_root = manifest_dir
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| anyhow!("Could not determine AdapterOS project root"))?
            .to_path_buf();

        let service_manager = project_root.join("scripts/service-manager.sh");
        let launch_script = project_root.join("launch.sh");

        Ok(Self {
            project_root,
            service_manager,
            launch_script,
        })
    }

    pub fn missing_prereqs(&self) -> Vec<String> {
        let mut missing = Vec::new();

        if !self.service_manager.exists() {
            missing.push(format!(
                "Missing service-manager script at {}",
                self.service_manager.display()
            ));
        }

        if !self.launch_script.exists() {
            missing.push(format!(
                "Missing launch script at {}",
                self.launch_script.display()
            ));
        }

        if !self
            .project_root
            .join("target/debug/adapteros-server")
            .exists()
        {
            missing.push(
                "Backend server binary not built. Run `cargo build -p adapteros-server`"
                    .to_string(),
            );
        }

        if which("pnpm").is_err() {
            missing.push(
                "`pnpm` is not installed. Install it to run the web UI (https://pnpm.io/install)"
                    .to_string(),
            );
        }

        missing
    }

    pub async fn start_all_services(&self) -> Result<ServiceCommandResult> {
        match self
            .run_service_manager(&["start", "all"], "scripts/service-manager.sh start all")
            .await
        {
            Ok(result) => Ok(result),
            Err(err) => {
                warn!(
                    error = ?err,
                    "Service manager failed to boot services, falling back to launch script"
                );
                if self.launch_script.exists() {
                    self.run_launch_script(&[], "./launch.sh").await
                } else {
                    Err(err)
                }
            }
        }
    }

    #[allow(dead_code)]
    pub async fn start_backend(&self) -> Result<ServiceCommandResult> {
        self.run_service_manager(
            &["start", "backend"],
            "scripts/service-manager.sh start backend",
        )
        .await
    }

    async fn run_service_manager(
        &self,
        args: &[&str],
        display: &str,
    ) -> Result<ServiceCommandResult> {
        if !self.service_manager.exists() {
            return Err(anyhow!(
                "Service manager script not found at {}",
                self.service_manager.display()
            ));
        }

        let mut cmd = Command::new("bash");
        cmd.arg(&self.service_manager);
        for arg in args {
            cmd.arg(arg);
        }

        self.spawn(cmd, display).await
    }

    async fn run_launch_script(
        &self,
        args: &[&str],
        display: &str,
    ) -> Result<ServiceCommandResult> {
        if !self.launch_script.exists() {
            return Err(anyhow!(
                "Launch script not found at {}",
                self.launch_script.display()
            ));
        }

        let mut cmd = Command::new("bash");
        cmd.arg(&self.launch_script);
        for arg in args {
            cmd.arg(arg);
        }

        self.spawn(cmd, display).await
    }

    async fn spawn(&self, mut cmd: Command, label: &str) -> Result<ServiceCommandResult> {
        cmd.current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        info!(command = %label, "Executing service command");
        let output = cmd
            .output()
            .await
            .map_err(|e| anyhow!("Failed to execute {}: {}", label, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        debug!(command = %label, %stdout, %stderr, "Service command finished");

        if !output.status.success() {
            let combined = ServiceCommandResult {
                command: label.to_string(),
                stdout,
                stderr,
            };
            return Err(anyhow!(
                "{} failed with status {:?}: {}",
                label,
                output.status.code(),
                combined.combined_output()
            ));
        }

        Ok(ServiceCommandResult {
            command: label.to_string(),
            stdout,
            stderr,
        })
    }
}
