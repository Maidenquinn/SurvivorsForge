//! Updates SurvivorsForge itself from the releases of `APP_REPO`.
//!
//! Release assets per platform, under a fixed name and (for launchers before 0.4.0) a versioned one:
//! - Windows: `SurvivorsForge-windows-x86_64.zip` / `SurvivorsForge-<version>-x86_64-pc-windows-msvc.zip`
//!   holding the `.exe`, which replaces the running executable.
//! - Linux: `SurvivorsForge-linux-x86_64.AppImage` / `SurvivorsForge-<version>-x86_64.AppImage`,
//!   which replaces the AppImage file itself (`$APPIMAGE`). A bare binary outside an AppImage is
//!   not updated in place.

use crate::github::{Client, Release};
use crate::{Error, Result};
use std::path::{Path, PathBuf};

pub const RELEASES_URL: &str = concat!("https://github.com/", "Rezondes/SurvivorsForge", "/releases/latest");

pub fn asset_name_for(windows: bool, version: &str) -> String {
    if windows {
        format!("{}-{version}-x86_64-pc-windows-msvc.zip", crate::APP_NAME)
    } else {
        format!("{}-{version}-x86_64.AppImage", crate::APP_NAME)
    }
}

pub fn asset_name(version: &str) -> String {
    asset_name_for(cfg!(windows), version)
}

/// The name that stays the same in every release; the versioned ones may go after 0.4.0.
pub fn stable_asset_name_for(windows: bool) -> String {
    if windows {
        format!("{}-windows-x86_64.zip", crate::APP_NAME)
    } else {
        format!("{}-linux-x86_64.AppImage", crate::APP_NAME)
    }
}

pub fn stable_asset_name() -> String {
    stable_asset_name_for(cfg!(windows))
}

/// The AppImage file to replace: `$APPIMAGE`, set by the AppImage runtime, if it points at a file.
pub fn appimage_path(appimage_env: Option<std::ffi::OsString>) -> Option<PathBuf> {
    appimage_env.map(PathBuf::from).filter(|p| p.is_file())
}

/// Whether this process can replace itself; otherwise the update banner links to the release.
pub fn can_replace_self() -> bool {
    cfg!(windows) || appimage_path(std::env::var_os("APPIMAGE")).is_some()
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
    let named = |wanted: String| release.assets.iter().find(|a| a.name == wanted);
    let asset = named(stable_asset_name()).or_else(|| named(asset_name(&version)))?;
    Some(Available { version, url: asset.url.clone() })
}

/// Downloads the release zip and swaps the running executable.
#[cfg(windows)]
pub fn apply(client: &Client, update: &Available, work_dir: &Path) -> Result<()> {
    use std::io::Read;
    let zip_path = work_dir.join(asset_name(&update.version));
    client.download(&update.url, &zip_path)?;
    let exe_name = format!("{}.exe", crate::APP_NAME);
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

/// Downloads the new AppImage next to the running one and renames it over it (same folder, so
/// the swap is atomic; the running process keeps its already opened file).
#[cfg(not(windows))]
pub fn apply(client: &Client, update: &Available, _work_dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let target = appimage_path(std::env::var_os("APPIMAGE"))
        .ok_or_else(|| Error::Io("SurvivorsForge only updates itself when it runs as an AppImage".into()))?;
    let new = target.with_extension("AppImage.new");
    let result = (|| -> Result<()> {
        client.download(&update.url, &new)?;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&new, &target)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&new);
    result
}

/// Starts the (now replaced) program and ends this process. An AppImage restarts from its file:
/// `current_exe` points into the AppImage's temporary mount, which goes away with this process.
pub fn restart() -> ! {
    let program = appimage_path(std::env::var_os("APPIMAGE")).or_else(|| std::env::current_exe().ok());
    if let Some(program) = program {
        let _ = std::process::Command::new(program).spawn();
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
        // A tester on a release candidate gets the final version; nobody goes back to a candidate.
        assert!(is_newer("0.4.0-rc.1", "v0.4.0"));
        assert!(is_newer("0.4.0-rc.1", "v0.4.0-rc.2"));
        assert!(!is_newer("0.4.0", "v0.4.0-rc.2"));
    }

    #[test]
    fn asset_name_per_platform() {
        assert_eq!(asset_name_for(true, "0.2.0"), "SurvivorsForge-0.2.0-x86_64-pc-windows-msvc.zip");
        assert_eq!(asset_name_for(false, "0.2.0"), "SurvivorsForge-0.2.0-x86_64.AppImage");
    }

    #[test]
    fn appimage_target_from_env() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("SurvivorsForge.AppImage");
        std::fs::write(&file, b"").unwrap();
        assert_eq!(appimage_path(Some(file.clone().into())), Some(file));
        assert_eq!(appimage_path(Some(dir.path().join("missing").into())), None);
        assert_eq!(appimage_path(None), None);
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

    #[test]
    fn available_from_the_fixed_name_first() {
        // Releases after 0.4.0 may carry only the fixed names; until then both are there.
        let fixed = stable_asset_name();
        assert_eq!(available("0.4.0", &release("v0.4.1", &[&fixed])).map(|a| a.url), Some(format!("https://dl/{fixed}")));
        let versioned = asset_name("0.4.1");
        assert_eq!(available("0.4.0", &release("v0.4.1", &[&versioned, &fixed])).map(|a| a.url), Some(format!("https://dl/{fixed}")));
        assert_eq!(stable_asset_name_for(true), "SurvivorsForge-windows-x86_64.zip");
        assert_eq!(stable_asset_name_for(false), "SurvivorsForge-linux-x86_64.AppImage");
    }
}
