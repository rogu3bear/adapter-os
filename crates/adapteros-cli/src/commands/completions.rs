//! Shell completion generation

use anyhow::Result;
use clap::Command;
use clap_complete::{generate, Shell};
use std::io;

/// Generate shell completion script
pub fn generate_completions(shell: Shell, cmd: &mut Command) -> Result<()> {
    let name = cmd.get_name().to_string();
    generate(shell, cmd, name, &mut io::stdout());
    Ok(())
}
