//! OS / distro detection and the package formats we can install.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Windows: zip extracted to %LOCALAPPDATA%\Programs.
    WindowsPortable,
    /// Debian / Ubuntu / Mint / Pop!_OS …
    Deb,
    /// Fedora / RHEL / openSUSE …
    Rpm,
    /// Arch / Manjaro / EndeavourOS …
    Arch,
    AppImage,
    Flatpak,
    /// Linux tar.gz extracted to ~/.local/share.
    Portable,
}

impl Format {
    pub const LINUX: [Format; 6] = [
        Format::Deb,
        Format::Rpm,
        Format::Arch,
        Format::AppImage,
        Format::Flatpak,
        Format::Portable,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Format::WindowsPortable => "windows",
            Format::Deb => "deb",
            Format::Rpm => "rpm",
            Format::Arch => "arch",
            Format::AppImage => "appimage",
            Format::Flatpak => "flatpak",
            Format::Portable => "portable",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::WindowsPortable => "Windows",
            Format::Deb => "Debian / Ubuntu (.deb)",
            Format::Rpm => "Fedora / openSUSE (.rpm)",
            Format::Arch => "Arch (pacman)",
            Format::AppImage => "AppImage",
            Format::Flatpak => "Flatpak",
            Format::Portable => "Portable (tar.gz)",
        }
    }

    pub fn parse(s: &str) -> Option<Format> {
        [Format::WindowsPortable]
            .into_iter()
            .chain(Format::LINUX)
            .find(|f| f.id().eq_ignore_ascii_case(s))
    }

    /// Needs root (system package manager)?
    pub fn needs_root(self) -> bool {
        matches!(self, Format::Deb | Format::Rpm | Format::Arch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux { id: String, like: Vec<String> },
    Other(String),
}

pub fn detect() -> Os {
    if cfg!(windows) {
        return Os::Windows;
    }
    if cfg!(target_os = "linux") {
        let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let (id, like) = parse_os_release(&text);
        return Os::Linux { id, like };
    }
    Os::Other(std::env::consts::OS.into())
}

pub fn parse_os_release(text: &str) -> (String, Vec<String>) {
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .map(|v| v.trim_matches('"').to_ascii_lowercase())
            .unwrap_or_default()
    };
    let like = get("ID_LIKE")
        .split_whitespace()
        .map(String::from)
        .collect();
    (get("ID"), like)
}

/// The format that fits this machine best, out of what the app publishes.
pub fn best_format(os: &Os, available: &[Format]) -> Option<Format> {
    let want = match os {
        Os::Windows => return Some(Format::WindowsPortable),
        Os::Linux { id, like } => {
            let is = |names: &[&str]| names.iter().any(|n| id == n || like.iter().any(|l| l == n));
            let native = if is(&["debian", "ubuntu"]) && which("dpkg").is_some() {
                Some(Format::Deb)
            } else if is(&["fedora", "rhel", "centos", "suse", "opensuse"])
                && which("rpm").is_some()
            {
                Some(Format::Rpm)
            } else if is(&["arch"]) && which("pacman").is_some() {
                Some(Format::Arch)
            } else {
                None
            };
            [
                native,
                Some(Format::AppImage),
                Some(Format::Flatpak),
                Some(Format::Portable),
            ]
        }
        Os::Other(_) => return None,
    };
    want.into_iter().flatten().find(|f| available.contains(f))
}

pub fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(cmd))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_native_format_from_os_release() {
        let (id, like) =
            parse_os_release("NAME=\"Linux Mint\"\nID=linuxmint\nID_LIKE=\"ubuntu debian\"\n");
        assert_eq!(id, "linuxmint");
        assert_eq!(like, vec!["ubuntu", "debian"]);
    }

    #[test]
    fn falls_back_to_appimage_on_unknown_distro() {
        let os = Os::Linux {
            id: "gentoo".into(),
            like: vec![],
        };
        assert_eq!(best_format(&os, &Format::LINUX), Some(Format::AppImage));
        assert_eq!(
            best_format(&os, &[Format::Portable]),
            Some(Format::Portable)
        );
    }

    #[test]
    fn windows_is_always_portable() {
        assert_eq!(
            best_format(&Os::Windows, &[]),
            Some(Format::WindowsPortable)
        );
    }

    #[test]
    fn format_ids_round_trip() {
        for f in Format::LINUX {
            assert_eq!(Format::parse(f.id()), Some(f));
        }
    }
}
