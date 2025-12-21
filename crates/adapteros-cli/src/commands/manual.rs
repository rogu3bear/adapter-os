//! Manual command with manpage generation and offline markdown

use anyhow::Result;
use clap::{Args, CommandFactory};
use clap_mangen::Man;
use std::io::{self, Write};

#[derive(Args, Clone)]
pub struct ManualArgs {
    /// Output format: man | md
    #[arg(long, default_value = "man")]
    pub format: String,

    /// Optional search token to highlight
    #[arg(long)]
    pub search: Option<String>,
}

const OFFLINE_MD: &str = include_str!("../../docs/aosctl_manual.md");

pub fn run_manual(args: ManualArgs) -> Result<()> {
    match args.format.as_str() {
        "man" => {
            // Use Cli::command() from clap's Command trait
            let cmd = crate::Cli::command();
            let man = Man::new(cmd);
            let mut buf = Vec::new();
            man.render(&mut buf)?;
            io::stdout().write_all(&buf)?;
        }
        "md" => {
            let text = if let Some(s) = args.search.as_ref() {
                // Lazy highlight
                OFFLINE_MD.replace(s, &format!("**{}**", s))
            } else {
                OFFLINE_MD.to_string()
            };
            println!("{}", text);
        }
        _ => eprintln!("Unknown format"),
    }
    Ok(())
}
