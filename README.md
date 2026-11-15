# Nullware Installer

Installs, updates and removes Nullware apps (Null YT) from their GitHub releases. Written in Rust, with a **GUI** and a **CLI**.

| Download | |
|---|---|
| `nullware-installer-windows-x64.exe` | GUI, Windows |
| `nullware-installer-cli-windows-x64.exe` | CLI, Windows |
| `nullware-installer-linux-x86_64` | GUI, Linux |
| `nullware-installer-cli-linux-x86_64` | CLI, Linux (no GUI libraries needed) |

They're on the [latest release](https://github.com/Nullware-Inc/Nullware-Installer/releases/latest), with SHA-256 checksums.

## Supported formats

| System | How it installs |
|---|---|
| **Windows 10/11** | Per-user install in `%LOCALAPPDATA%\Programs\Nullware`, with no admin rights needed. Adds a Start menu shortcut and an entry in *Apps & features* so you can uninstall from there. |
| **Debian / Ubuntu / Mint** | `.deb` through `apt` |
| **Fedora / openSUSE** | `.rpm` through `dnf` or `zypper` |
| **Arch / Manjaro / EndeavourOS** | `.pkg.tar.zst` through `pacman` |
| **Any Linux** | **AppImage** or a **portable** tar.gz in `~/.local/share/nullware`, both with a menu entry and a `~/.local/bin` link. Or **Flatpak** (`--user`, with the GNOME runtime from Flathub). |
| **Android** | Not through this installer. Use [Obtainium](https://apps.obtainium.imranr.dev/redirect?r=obtainium://add/https://github.com/Nullware-Inc/Null-YT-Client-Mobile-Dart) or the APKs on the [mobile releases](https://github.com/Nullware-Inc/Null-YT-Client-Mobile-Dart/releases/latest). |

On Linux it reads `/etc/os-release` and picks the native format automatically. Override it with `--format`.

## CLI

```bash
nullware-installer-cli list                      # apps, installed + latest versions
nullware-installer-cli install                   # Null YT, best format for this system
nullware-installer-cli install nullyt --format appimage
nullware-installer-cli install nullyt-rust       # the Rust edition
nullware-installer-cli install nullyt --version 0.1.0 -y
nullware-installer-cli update                    # update everything installed
nullware-installer-cli uninstall nullyt
```

The GUI binary accepts the same commands. Started with no arguments, it opens its window.

## Build

```bash
cargo build --release --bin nullware-installer                         # GUI
cargo build --release --no-default-features --bin nullware-installer-cli  # CLI only
```

On Linux the GUI build needs `libfontconfig1-dev libxkbcommon-dev`.
