//! Which apps the installer knows about, and how their release assets are named.

use crate::platform::Format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    /// Stable id used on the command line and in the install record.
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// GitHub repo (owner/name) whose releases hold the builds.
    pub repo: &'static str,
    /// Executable inside a portable/zip build.
    pub exe_windows: &'static str,
    pub exe_linux: &'static str,
    /// Formats this app publishes on Linux (Windows is always a portable zip).
    pub linux_formats: &'static [Format],
    /// Flatpak app id, when a Flatpak is published.
    pub flatpak_id: Option<&'static str>,
    /// Native package name (deb/rpm/pacman), when packages are published.
    pub package_name: Option<&'static str>,
}

pub const APPS: &[App] = &[
    App {
        id: "nullyt",
        name: "Null YT",
        description: "The full Null YT desktop app: liquid glass or solid, YouTube sync, built-in player.",
        repo: "Nullware-Inc/Null-YT-Client-Desktop-Dart",
        exe_windows: "nullyt_desktop.exe",
        exe_linux: "nullyt_desktop",
        linux_formats: &[
            Format::Deb,
            Format::Rpm,
            Format::Arch,
            Format::AppImage,
            Format::Flatpak,
            Format::Portable,
        ],
        flatpak_id: Some("inc.nullware.NullYT"),
        package_name: Some("nullyt"),
    },
    App {
        id: "nullyt-rust",
        name: "Null YT (Rust edition)",
        description: "Lightweight native client. Plays videos in mpv.",
        repo: "Nullware-Inc/Null-YT-Client-Desktop-Rust",
        exe_windows: "nullyt.exe",
        exe_linux: "nullyt",
        linux_formats: &[Format::Portable],
        flatpak_id: None,
        package_name: None,
    },
];

pub fn find(id: &str) -> Option<&'static App> {
    APPS.iter().find(|a| a.id.eq_ignore_ascii_case(id))
}

impl App {
    /// Does this release asset carry this app in this format?
    pub fn asset_matches(&self, name: &str, format: Format) -> bool {
        let n = name.to_ascii_lowercase();
        match format {
            Format::WindowsPortable => n.contains("windows") && n.ends_with(".zip"),
            Format::Portable => n.contains("linux") && n.ends_with(".tar.gz"),
            Format::Deb => n.ends_with("_amd64.deb"),
            Format::Rpm => n.ends_with(".x86_64.rpm"),
            Format::Arch => n.ends_with(".pkg.tar.zst"),
            Format::AppImage => n.ends_with(".appimage"),
            Format::Flatpak => n.ends_with(".flatpak"),
        }
    }
}
