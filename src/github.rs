//! Reading releases from GitHub and downloading assets.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::Path;

const UA: &str = concat!("nullware-installer/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

impl Release {
    /// "v0.1.0" → "0.1.0"
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches('v')
    }
}

/// Latest release, or a specific version ("0.1.0" / "v0.1.0").
pub fn release(repo: &str, version: Option<&str>) -> Result<Release> {
    let url = match version {
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
        Some(v) => format!(
            "https://api.github.com/repos/{repo}/releases/tags/v{}",
            v.trim_start_matches('v')
        ),
    };
    let mut res = ureq::get(&url)
        .header("User-Agent", UA)
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("couldn't reach GitHub ({repo})"))?;
    let text = res
        .body_mut()
        .read_to_string()
        .context("read GitHub reply")?;
    let release: Release = serde_json::from_str(&text).context("unexpected reply from GitHub")?;
    Ok(release)
}

/// Streams an asset to `dest`, reporting progress (0.0–1.0).
pub fn download(asset: &Asset, dest: &Path, progress: &mut dyn FnMut(f32)) -> Result<()> {
    let res = ureq::get(&asset.browser_download_url)
        .header("User-Agent", UA)
        .call()
        .with_context(|| format!("download {}", asset.name))?;
    let total = asset.size.max(1);
    let mut reader = res.into_body().into_reader();
    let tmp = dest.with_extension("part");
    let mut file =
        std::fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress((done as f32 / total as f32).min(1.0));
    }
    drop(file);
    if done != asset.size {
        bail!(
            "download of {} was incomplete ({done} of {} bytes)",
            asset.name,
            asset.size
        );
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}
