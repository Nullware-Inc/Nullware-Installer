//! nullware-installer-cli: terminal-only installer (no GUI toolkit).

use clap::Parser;
use nullware_installer::cli::{Cli, run};

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
