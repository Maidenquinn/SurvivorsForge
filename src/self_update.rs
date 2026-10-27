//! Updates SurvivorsForge itself from the releases of `APP_REPO`.
//! Release asset: `SurvivorsForge-<version>-<target>.zip` holding the executable.

use crate::github::{Client, Release};
use crate::{Error, Result};
use std::io::Read;
use std::path::Path;

pub const TARGET: &str = if cfg!(windows) { "x86_64-pc-windows-msvc" } else { "x86_64-unknown-linux-gnu" };

pub fn asset_name(version: &str) -> String {
    format!("{}-{version}-{TARGET}.zip", crate::APP_NAME)
}

fn parse(version: &str) -> Option<semver::Version> {
    semver::Version::parse(version.trim().trim_start_matches(['v', 'V'])).ok()
}

pub fn is_newer(current: &str, tag: &str) -> bool {
    matches!((parse(current), parse(tag)), (Some(c), Some(t)) if t > c)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Available {
    pub version: String,
    pub url: String,
}

/// The newer version from `release` with an asset for this platform, if any.
pub fn available(current: &str, release: &Release) -> Option<Available> {
    if !is_newer(current, &release.tag) {
        return None;
    }
    let version = parse(&release.tag)?.to_string();
    let wanted = asset_name(&version);
    let asset = release.assets.iter().find(|a| a.name == wanted)?;
    Some(Available { version, url: asset.url.clone() })
}

/// Downloads the release zip, swaps the running executable and returns once the new one is in place.
pub fn apply(client: &Client, update: &Available, work_dir: &Path) -> Result<()> {
    let zip_path = work_dir.join(asset_name(&update.version));
    client.download(&update.url, &zip_path)?;
    let exe_name = if cfg!(windows) { format!("{}.exe", crate::APP_NAME) } else { crate::APP_NAME.to_string() };
    let new_exe = work_dir.join(format!("new-{exe_name}"));
    let result = (|| -> Result<()> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&zip_path)?).map_err(|e| Error::Zip(e.to_string()))?;
        let mut file = zip.by_name(&exe_name).map_err(|e| Error::Zip(e.to_string()))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        std::fs::write(&new_exe, bytes)?;
        self_replace::self_replace(&new_exe)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_file(&new_exe);
    result
}

/// Starts the (now replaced) executable and ends this process.
pub fn restart() -> ! {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Asset;

    fn release(tag: &str, assets: &[&str]) -> Release {
        Release {
            tag: tag.into(),
            assets: assets.iter().map(|n| Asset { name: n.to_string(), url: format!("https://dl/{n}") }).collect(),
        }
    }

    #[test]
    fn newer_version_shows_banner() {
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(!is_newer("0.1.0", "v0.1.0"));
        assert!(!is_newer("0.1.0", "v0.0.9"));
        assert!(!is_newer("0.1.0", "nightly"));
    }

    #[test]
    fn asset_name_for_target() {
        assert_eq!(asset_name("0.2.0"), format!("SurvivorsForge-0.2.0-{TARGET}.zip"));
    }

    #[test]
    fn available_needs_matching_asset() {
        let name = asset_name("0.2.0");
        assert_eq!(
            available("0.1.0", &release("v0.2.0", &[&name])),
            Some(Available { version: "0.2.0".into(), url: format!("https://dl/{name}") })
        );
        assert_eq!(available("0.1.0", &release("v0.2.0", &["other.zip"])), None);
        assert_eq!(available("0.2.0", &release("v0.2.0", &[&name])), None);
    }
}
