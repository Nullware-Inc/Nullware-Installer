//! nullware-installer: graphical installer. With arguments it behaves exactly
//! like nullware-installer-cli (that's how the Windows uninstall entry calls it).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use clap::Parser;
use nullware_installer::catalog::{self, APPS, App};
use nullware_installer::cli::{Cli, run};
use nullware_installer::install::{self, Reporter};
use nullware_installer::platform;
use nullware_installer::state::State;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::rc::Rc;

slint::include_modules!();

/// A unit of work run off the UI thread (install / uninstall).
type Job = Box<dyn FnOnce(&mut UiReporter) -> anyhow::Result<()> + Send>;

fn main() {
    if std::env::args_os().len() > 1 {
        if let Err(e) = run(Cli::parse()) {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = gui() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn row(app: &App, latest: &str) -> AppRow {
    let state = State::load();
    let formats = install::available_formats(app);
    let default = install::default_format(app);
    AppRow {
        id: app.id.into(),
        name: app.name.into(),
        description: app.description.into(),
        latest: latest.into(),
        installed: state
            .get(app.id)
            .map(|i| i.version.clone())
            .unwrap_or_default()
            .into(),
        formats: ModelRc::new(VecModel::from(
            formats
                .iter()
                .map(|f| SharedString::from(f.label()))
                .collect::<Vec<_>>(),
        )),
        format_index: default
            .and_then(|d| formats.iter().position(|f| *f == d))
            .unwrap_or(0) as i32,
        can_install: !formats.is_empty() && latest != "unavailable",
    }
}

fn refresh(ui: &InstallerWindow, latest: &[String]) {
    let rows: Vec<AppRow> = APPS.iter().zip(latest).map(|(a, l)| row(a, l)).collect();
    ui.set_apps(ModelRc::new(Rc::new(VecModel::from(rows))));
}

/// Forwards engine progress to the window from the worker thread.
struct UiReporter(Weak<InstallerWindow>);

impl Reporter for UiReporter {
    fn status(&mut self, msg: &str) {
        let msg = SharedString::from(msg);
        let _ = self.0.upgrade_in_event_loop(move |ui| ui.set_status(msg));
    }
    fn progress(&mut self, fraction: Option<f32>) {
        let v = fraction.unwrap_or(2.0);
        let _ = self.0.upgrade_in_event_loop(move |ui| ui.set_progress(v));
    }
}

fn latest_versions() -> Vec<String> {
    APPS.iter()
        .map(|a| {
            nullware_installer::github::release(a.repo, None)
                .map(|r| r.version().to_string())
                .unwrap_or_else(|_| "unavailable".into())
        })
        .collect()
}

fn gui() -> Result<(), slint::PlatformError> {
    let ui = InstallerWindow::new()?;
    ui.set_system(
        match platform::detect() {
            platform::Os::Windows => {
                "Windows · installs for your user, no admin needed".to_string()
            }
            platform::Os::Linux { id, .. } => format!("Linux ({id})"),
            platform::Os::Other(o) => format!("{o} (not supported yet)"),
        }
        .into(),
    );
    let latest = Rc::new(std::cell::RefCell::new(vec!["…".to_string(); APPS.len()]));
    refresh(&ui, &latest.borrow());
    ui.set_progress(2.0);

    // Fetch latest versions without blocking the window.
    {
        let weak = ui.as_weak();
        let latest = latest.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(latest_versions());
        });
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(100),
            move || {
                if let Ok(v) = rx.try_recv() {
                    let ui = weak.unwrap();
                    *latest.borrow_mut() = v;
                    refresh(&ui, &latest.borrow());
                    ui.set_progress(-1.0);
                    ui.set_status("Pick an app to install.".into());
                }
            },
        );
        std::mem::forget(timer); // lives for the whole app
    }

    let work = {
        let weak = ui.as_weak();
        let latest = latest.clone();
        move |job: Job| {
            let ui = weak.unwrap();
            ui.set_busy(true);
            let weak = weak.clone();
            let versions = latest.borrow().clone();
            std::thread::spawn(move || {
                let mut r = UiReporter(weak.clone());
                let result = job(&mut r);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_busy(false);
                    ui.set_progress(-1.0);
                    if let Err(e) = result {
                        ui.set_status(format!("Failed: {e:#}").into());
                    }
                    refresh(&ui, &versions);
                });
            });
        }
    };

    ui.on_install({
        let work = work.clone();
        move |id, format_index| {
            let Some(app) = catalog::find(&id) else {
                return;
            };
            let format = install::available_formats(app)
                .get(format_index as usize)
                .copied();
            work(Box::new(move |r| {
                install::install(app, format, None, r).map(|_| ())
            }));
        }
    });
    ui.on_uninstall({
        let work = work.clone();
        move |id| {
            let Some(app) = catalog::find(&id) else {
                return;
            };
            work(Box::new(move |r| install::uninstall(app, r)));
        }
    });

    ui.run()
}
