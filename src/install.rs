//! Installing, updating and uninstalling, per format.

use crate::catalog::App;
use crate::github;
use crate::platform::{self, Format, Os};
use crate::state::{Installed, State};
use anyhow::{Context, Result, anyhow, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where progress goes: a terminal progress bar or the GUI.
pub trait Reporter {
    fn status(&mut self, msg: &str);
    /// `None` = indeterminate (spinner).
    fn progress(&mut self, fraction: Option<f32>);
}

/// Formats this app can be installed as on this machine.
pub fn available_formats(app: &App) -> Vec<Format> {
    match platform::detect() {
        Os::Windows => vec![Format::WindowsPortable],
        Os::Linux { .. } => app.linux_formats.to_vec(),
        Os::Other(_) => vec![],
    }
}

pub fn default_format(app: &App) -> Option<Format> {
    platform::best_format(&platform::detect(), &available_formats(app))
}

pub fn install(
    app: &App,
    format: Option<Format>,
    version: Option<&str>,
    r: &mut dyn Reporter,
) -> Result<Installed> {
    let os = platform::detect();
    if let Os::Other(name) = &os {
        bail!("{name} isn't supported yet. Null YT runs on Windows, Linux and Android.");
    }
    let format = match format {
        Some(f) => f,
        None => default_format(app)
            .ok_or_else(|| anyhow!("no build of {} fits this system", app.name))?,
    };
    if !available_formats(app).contains(&format) {
        bail!(
            "{} isn't published as {} for this system",
            app.name,
            format.label()
        );
    }

    r.status(&format!("Looking up {} on GitHub…", app.name));
    r.progress(None);
    let release = github::release(app.repo, version)?;
    let asset = release
        .assets
        .iter()
        .find(|a| app.asset_matches(&a.name, format))
        .ok_or_else(|| {
            anyhow!(
                "release {} has no {} build",
                release.tag_name,
                format.label()
            )
        })?;

    let tmp = std::env::temp_dir().join("nullware-installer");
    std::fs::create_dir_all(&tmp)?;
    let file = tmp.join(&asset.name);
    r.status(&format!(
        "Downloading {} ({:.1} MB)…",
        asset.name,
        asset.size as f64 / 1e6
    ));
    github::download(asset, &file, &mut |p| r.progress(Some(p)))?;

    r.status(&format!(
        "Installing {} {} ({})…",
        app.name,
        release.version(),
        format.label()
    ));
    r.progress(None);

    // Updating in place: remove what the previous version left behind first.
    let mut state = State::load();
    if let Some(prev) = state.get(app.id).cloned()
        && (prev.format != format || !format.needs_root())
    {
        remove_files(&prev.paths);
    }

    let paths = match format {
        Format::WindowsPortable => windows::install(app, &file, release.version())?,
        Format::Deb => {
            linux::root(&["apt-get", "install", "-y", &abs(&file)])?;
            vec![]
        }
        Format::Rpm => {
            if platform::which("dnf").is_some() {
                linux::root(&["dnf", "install", "-y", &abs(&file)])?
            } else {
                linux::root(&[
                    "zypper",
                    "--non-interactive",
                    "install",
                    "--allow-unsigned-rpm",
                    &abs(&file),
                ])?
            }
            vec![]
        }
        Format::Arch => {
            linux::root(&["pacman", "-U", "--noconfirm", &abs(&file)])?;
            vec![]
        }
        Format::Flatpak => {
            linux::flatpak_install(&file)?;
            vec![]
        }
        Format::AppImage => linux::install_appimage(app, &file)?,
        Format::Portable => linux::install_portable(app, &file)?,
    };
    let _ = std::fs::remove_file(&file);

    let entry = Installed {
        app: app.id.into(),
        version: release.version().into(),
        format,
        paths,
    };
    state.put(entry.clone());
    state.save().context("save install record")?;
    r.progress(Some(1.0));
    r.status(&format!("{} {} is installed.", app.name, entry.version));
    Ok(entry)
}

pub fn uninstall(app: &App, r: &mut dyn Reporter) -> Result<()> {
    let mut state = State::load();
    let entry = state
        .get(app.id)
        .cloned()
        .ok_or_else(|| anyhow!("{} wasn't installed with this installer", app.name))?;
    r.status(&format!("Removing {}…", app.name));
    r.progress(None);
    let pkg = app.package_name.unwrap_or(app.id);
    match entry.format {
        Format::WindowsPortable => windows::uninstall(app, &entry.paths)?,
        Format::Deb => linux::root(&["apt-get", "remove", "-y", pkg])?,
        Format::Rpm => {
            if platform::which("dnf").is_some() {
                linux::root(&["dnf", "remove", "-y", pkg])?
            } else {
                linux::root(&["zypper", "--non-interactive", "remove", pkg])?
            }
        }
        Format::Arch => linux::root(&["pacman", "-R", "--noconfirm", pkg])?,
        Format::Flatpak => {
            let id = app.flatpak_id.ok_or_else(|| anyhow!("no flatpak id"))?;
            run(&[
                "flatpak",
                "uninstall",
                "--user",
                "-y",
                "--noninteractive",
                id,
            ])?
        }
        Format::AppImage | Format::Portable => remove_files(&entry.paths),
    }
    state.remove(app.id);
    state.save()?;
    r.progress(Some(1.0));
    r.status(&format!("{} was removed.", app.name));
    Ok(())
}

/// Installed version vs. latest release, if an update exists.
pub fn check_update(app: &App) -> Result<Option<String>> {
    let Some(entry) = State::load().get(app.id).cloned() else {
        return Ok(None);
    };
    let latest = github::release(app.repo, None)?;
    Ok((latest.version() != entry.version).then(|| latest.version().to_string()))
}

fn abs(p: &Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn remove_files(paths: &[PathBuf]) {
    for p in paths {
        let _ = if p.is_dir() {
            std::fs::remove_dir_all(p)
        } else {
            std::fs::remove_file(p)
        };
    }
}

fn run(args: &[&str]) -> Result<()> {
    let status = Command::new(args[0])
        .args(&args[1..])
        .status()
        .with_context(|| format!("couldn't run {}", args[0]))?;
    if !status.success() {
        bail!("`{}` failed ({status})", args.join(" "));
    }
    Ok(())
}

// ── Linux ─────────────────────────────────────────────────────────────────────

mod linux {
    use super::*;
    use std::io::IsTerminal;

    /// Runs a package-manager command as root: directly if we are root, with
    /// sudo in a terminal, otherwise with pkexec (graphical password prompt).
    pub fn root(args: &[&str]) -> Result<()> {
        let is_root = Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false);
        if is_root {
            return run(args);
        }
        let elevate = if std::io::stdin().is_terminal() && platform::which("sudo").is_some() {
            "sudo"
        } else if platform::which("pkexec").is_some() {
            "pkexec"
        } else if platform::which("sudo").is_some() {
            "sudo"
        } else {
            bail!("installing system packages needs sudo or pkexec");
        };
        let mut full = vec![elevate];
        full.extend_from_slice(args);
        run(&full)
    }

    pub fn flatpak_install(file: &Path) -> Result<()> {
        if platform::which("flatpak").is_none() {
            bail!("Flatpak isn't installed. Install it from your distro, or pick another format.");
        }
        // The bundle needs the GNOME runtime, which comes from Flathub.
        run(&[
            "flatpak",
            "remote-add",
            "--user",
            "--if-not-exists",
            "flathub",
            "https://dl.flathub.org/repo/flathub.flatpakrepo",
        ])?;
        run(&[
            "flatpak",
            "install",
            "--user",
            "-y",
            "--noninteractive",
            &abs(file),
        ])
    }

    fn home() -> Result<PathBuf> {
        dirs::home_dir().ok_or_else(|| anyhow!("no home directory"))
    }

    fn app_dir(app: &App) -> Result<PathBuf> {
        Ok(home()?.join(".local/share/nullware").join(app.id))
    }

    /// ~/.local/bin/<id> link + a launcher entry so it shows up in app menus.
    fn integrate(app: &App, exe: &Path) -> Result<Vec<PathBuf>> {
        let bin_dir = home()?.join(".local/bin");
        std::fs::create_dir_all(&bin_dir)?;
        let link = bin_dir.join(app.id);
        let _ = std::fs::remove_file(&link);
        #[cfg(unix)]
        std::os::unix::fs::symlink(exe, &link)?;

        let apps = home()?.join(".local/share/applications");
        std::fs::create_dir_all(&apps)?;
        let desktop = apps.join(format!("inc.nullware.{}.desktop", app.id));
        std::fs::write(
            &desktop,
            format!(
                "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec=\"{}\" %u\nIcon=video-x-generic\nTerminal=false\nCategories=AudioVideo;Video;Player;\n",
                app.name,
                app.description,
                exe.display()
            ),
        )?;
        Ok(vec![link, desktop])
    }

    pub fn install_appimage(app: &App, file: &Path) -> Result<Vec<PathBuf>> {
        let dir = app_dir(app)?;
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join(format!("{}.AppImage", app.id));
        std::fs::copy(file, &dest)?;
        make_executable(&dest)?;
        let mut paths = vec![dir];
        paths.extend(integrate(app, &dest)?);
        Ok(paths)
    }

    pub fn install_portable(app: &App, file: &Path) -> Result<Vec<PathBuf>> {
        let dir = app_dir(app)?;
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let gz = flate2::read::GzDecoder::new(std::fs::File::open(file)?);
        tar::Archive::new(gz)
            .unpack(&dir)
            .context("extract archive")?;
        let exe = dir.join(app.exe_linux);
        if !exe.exists() {
            bail!("{} not found in the archive", app.exe_linux);
        }
        make_executable(&exe)?;
        let ytdlp = dir.join("yt-dlp");
        if ytdlp.exists() {
            make_executable(&ytdlp)?;
        }
        let mut paths = vec![dir];
        paths.extend(integrate(app, &exe)?);
        Ok(paths)
    }

    fn make_executable(_p: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(_p, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    }
}

// ── Windows ───────────────────────────────────────────────────────────────────

mod windows {
    use super::*;

    pub fn install_dir(app: &App) -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Programs")
            .join("Nullware")
            .join(app.id)
    }

    fn shortcut(app: &App) -> Option<PathBuf> {
        dirs::data_dir().map(|d| {
            d.join("Microsoft\\Windows\\Start Menu\\Programs")
                .join(format!("{}.lnk", app.name))
        })
    }

    /// Per-user install (no admin): extract to %LOCALAPPDATA%\Programs,
    /// Start menu shortcut, and an "Apps & features" entry.
    pub fn install(app: &App, zip: &Path, version: &str) -> Result<Vec<PathBuf>> {
        let dir = install_dir(app);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        zip::ZipArchive::new(std::fs::File::open(zip)?)
            .context("open zip")?
            .extract(&dir)
            .context("extract zip")?;
        let exe = dir.join(app.exe_windows);
        if !exe.exists() {
            bail!("{} not found in the archive", app.exe_windows);
        }

        // A copy of ourselves, so "Uninstall" works even if the installer is deleted.
        let uninstaller = dir.join("nullware-uninstall.exe");
        if let Ok(me) = std::env::current_exe() {
            let _ = std::fs::copy(me, &uninstaller);
        }

        let mut paths = vec![dir.clone()];
        if let Some(lnk) = shortcut(app) {
            let script = format!(
                "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.Save()",
                lnk.display(),
                exe.display(),
                dir.display()
            );
            if run(&[
                "powershell",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &script,
            ])
            .is_ok()
            {
                paths.push(lnk);
            }
        }
        register(app, &dir, &exe, &uninstaller, version)?;
        Ok(paths)
    }

    #[cfg(windows)]
    fn uninstall_key(app: &App) -> String {
        format!(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Nullware.{}",
            app.id
        )
    }

    #[cfg(windows)]
    fn register(
        app: &App,
        dir: &Path,
        exe: &Path,
        uninstaller: &Path,
        version: &str,
    ) -> Result<()> {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(uninstall_key(app))?;
        key.set_value("DisplayName", &app.name)?;
        key.set_value("DisplayVersion", &version)?;
        key.set_value("Publisher", &"Nullware")?;
        key.set_value("InstallLocation", &dir.display().to_string())?;
        key.set_value("DisplayIcon", &exe.display().to_string())?;
        key.set_value(
            "UninstallString",
            &format!("\"{}\" uninstall {} --yes", uninstaller.display(), app.id),
        )?;
        key.set_value("NoModify", &1u32)?;
        key.set_value("NoRepair", &1u32)?;
        Ok(())
    }

    #[cfg(not(windows))]
    fn register(_: &App, _: &Path, _: &Path, _: &Path, _: &str) -> Result<()> {
        Ok(())
    }

    /// We can't delete our own running exe, so a hidden cmd removes the folder
    /// a moment after we exit. `ping` is the delay because `timeout` refuses to
    /// run without a console, and the command line is passed verbatim
    /// (`raw_arg`) because Rust's argument quoting isn't what cmd expects.
    #[cfg(windows)]
    fn schedule_dir_removal(dir: &Path) -> Result<()> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("cmd")
            .raw_arg(format!(
                "/C ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"",
                dir.display()
            ))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
        Ok(())
    }

    #[cfg(not(windows))]
    fn schedule_dir_removal(dir: &Path) -> Result<()> {
        let _ = std::fs::remove_dir_all(dir);
        Ok(())
    }

    pub fn uninstall(app: &App, paths: &[PathBuf]) -> Result<()> {
        #[cfg(windows)]
        {
            use winreg::RegKey;
            use winreg::enums::HKEY_CURRENT_USER;
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(uninstall_key(app));
        }
        let dir = install_dir(app);
        let me = std::env::current_exe().ok();
        let running_from_inside = me.as_ref().is_some_and(|m| m.starts_with(&dir));
        let others: Vec<PathBuf> = paths.iter().filter(|p| **p != dir).cloned().collect();
        remove_files(&others);
        if running_from_inside {
            schedule_dir_removal(&dir)?;
        } else {
            let _ = std::fs::remove_dir_all(&dir);
        }
        Ok(())
    }
}
