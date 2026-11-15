//! Command-line interface (used by both binaries).

use crate::catalog::{self, APPS, App};
use crate::install::{self, Reporter};
use crate::platform::{self, Format};
use crate::state::State;
use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{IsTerminal, Write};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "nullware-installer",
    version,
    about = "Install, update and remove Nullware apps"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Show available apps, installed versions and the latest releases
    List,
    /// Install an app (or reinstall it)
    Install {
        /// App id: nullyt | nullyt-rust
        #[arg(default_value = "nullyt")]
        app: String,
        /// windows | deb | rpm | arch | appimage | flatpak | portable (default: best for this system)
        #[arg(long, short)]
        format: Option<String>,
        /// Install a specific version (default: latest)
        #[arg(long)]
        version: Option<String>,
        /// Don't ask for confirmation
        #[arg(long, short)]
        yes: bool,
    },
    /// Update installed apps to the latest release
    Update {
        /// Only this app (default: all installed)
        app: Option<String>,
        #[arg(long, short)]
        yes: bool,
    },
    /// Remove an app
    Uninstall {
        app: String,
        #[arg(long, short)]
        yes: bool,
    },
}

struct Bar(ProgressBar);

impl Bar {
    fn new() -> Self {
        let pb = ProgressBar::new(1000);
        pb.enable_steady_tick(Duration::from_millis(100));
        Self(pb)
    }
}

impl Reporter for Bar {
    fn status(&mut self, msg: &str) {
        self.0.set_message(msg.to_string());
    }
    fn progress(&mut self, fraction: Option<f32>) {
        match fraction {
            Some(f) => {
                self.0.set_style(
                    ProgressStyle::with_template("{bar:30.magenta/black} {percent:>3}%  {msg}")
                        .unwrap()
                        .progress_chars("━━─"),
                );
                self.0.set_position((f * 1000.0) as u64);
            }
            None => self
                .0
                .set_style(ProgressStyle::with_template("{spinner:.magenta} {msg}").unwrap()),
        }
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}

fn app(id: &str) -> Result<&'static App> {
    catalog::find(id).ok_or_else(|| {
        let ids: Vec<_> = APPS.iter().map(|a| a.id).collect();
        anyhow!("unknown app \"{id}\" (available: {})", ids.join(", "))
    })
}

fn confirm(question: &str, yes: bool) -> bool {
    if yes || !std::io::stdin().is_terminal() {
        return true;
    }
    print!("{question} [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    !matches!(line.trim().to_ascii_lowercase().as_str(), "n" | "no")
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::List => {
            let state = State::load();
            println!("System: {:?}\n", platform::detect());
            for a in APPS {
                let latest = crate::github::release(a.repo, None)
                    .map(|r| r.version().to_string())
                    .unwrap_or_else(|_| "unavailable".into());
                let installed = state
                    .get(a.id)
                    .map(|i| format!("{} ({})", i.version, i.format.label()))
                    .unwrap_or_else(|| "not installed".into());
                let formats: Vec<_> = install::available_formats(a)
                    .iter()
                    .map(|f| f.id())
                    .collect();
                println!("{}  [{}]", a.name, a.id);
                println!("  {}", a.description);
                println!("  latest: {latest}   installed: {installed}");
                println!("  formats here: {}\n", formats.join(", "));
            }
            Ok(())
        }
        Command::Install {
            app: id,
            format,
            version,
            yes,
        } => {
            let a = app(&id)?;
            let format = format
                .map(|f| Format::parse(&f).ok_or_else(|| anyhow!("unknown format \"{f}\"")))
                .transpose()?;
            let chosen = format.or_else(|| install::default_format(a));
            let label = chosen.map(|f| f.label()).unwrap_or("?");
            if !confirm(&format!("Install {} as {label}?", a.name), yes) {
                return Ok(());
            }
            let entry = install::install(a, format, version.as_deref(), &mut Bar::new())?;
            println!(
                "✓ {} {} installed ({}).",
                a.name,
                entry.version,
                entry.format.label()
            );
            Ok(())
        }
        Command::Update { app: only, yes } => {
            let state = State::load();
            let targets: Vec<_> = match &only {
                Some(id) => vec![app(id)?],
                None => APPS.iter().filter(|a| state.get(a.id).is_some()).collect(),
            };
            if targets.is_empty() {
                println!("Nothing installed yet. Try: nullware-installer install nullyt");
            }
            for a in targets {
                match install::check_update(a)? {
                    None => println!("✓ {} is up to date.", a.name),
                    Some(v) => {
                        if confirm(&format!("Update {} to {v}?", a.name), yes) {
                            let fmt = state.get(a.id).map(|i| i.format);
                            install::install(a, fmt, Some(&v), &mut Bar::new())?;
                            println!("✓ {} updated to {v}.", a.name);
                        }
                    }
                }
            }
            Ok(())
        }
        Command::Uninstall { app: id, yes } => {
            let a = app(&id)?;
            if !confirm(&format!("Remove {}?", a.name), yes) {
                return Ok(());
            }
            install::uninstall(a, &mut Bar::new())?;
            println!("✓ {} removed.", a.name);
            Ok(())
        }
    }
}
