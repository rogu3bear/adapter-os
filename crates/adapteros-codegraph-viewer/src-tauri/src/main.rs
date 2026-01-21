//! CodeGraph Viewer - Interactive graph visualization for code analysis
//!
//! Tauri desktop application for viewing and analyzing CodeGraph databases.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod diff;
mod types;

use commands::*;
use tracing_subscriber;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_graph,
            search_symbols,
            get_symbol_details,
            get_neighbors,
            load_diff,
            open_source_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

