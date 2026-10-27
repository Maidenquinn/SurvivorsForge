use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

const FILE: &str = "config.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub addons_dir: PathBuf,
}

/// `%APPDATA%\SurvivorsForge` on Windows, `~/.config/SurvivorsForge` on Linux.
/// `SURVIVORSFORGE_DATA_DIR` overrides it, for testing against a copy of an AddOns folder.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SURVIVORSFORGE_DATA_DIR") {
        return dir.into();
    }
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join(crate::APP_NAME))
        .unwrap_or_else(|| PathBuf::from(crate::APP_NAME))
}

impl Config {
    pub fn load(dir: &Path) -> Option<Config> {
        let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).expect("config serializes");
        crate::write_atomic(&dir.join(FILE), &json)
    }
}

#[derive(Debug, PartialEq)]
pub enum DirError {
    NotFound,
    NotADirectory,
    NotWritable,
}

impl fmt::Display for DirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DirError::NotFound => "The folder does not exist.",
            DirError::NotADirectory => "The path is not a folder.",
            DirError::NotWritable => "The folder is not writable.",
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum DirWarning {
    NotNamedAddOns,
}

impl fmt::Display for DirWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("The folder is not named \"AddOns\". Is this really the game's AddOns folder?")
    }
}

pub fn validate_addons_dir(path: &Path) -> Result<Option<DirWarning>, DirError> {
    if !path.exists() {
        return Err(DirError::NotFound);
    }
    if !path.is_dir() {
        return Err(DirError::NotADirectory);
    }
    let probe = path.join(format!(".survivorsforge-probe-{}", crate::unique_name()));
    std::fs::write(&probe, b"").map_err(|_| DirError::NotWritable)?;
    let _ = std::fs::remove_file(&probe);
    let named_addons = path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("AddOns"));
    Ok((!named_addons).then_some(DirWarning::NotNamedAddOns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_none_without_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(dir.path()), None);
    }

    #[test]
    fn validate_rejects_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(validate_addons_dir(&dir.path().join("nope")), Err(DirError::NotFound));
    }

    #[test]
    fn validate_accepts_addons_and_warns_on_unusual_name() {
        let dir = tempfile::tempdir().unwrap();
        let addons = dir.path().join("AddOns");
        let foo = dir.path().join("foo");
        std::fs::create_dir(&addons).unwrap();
        std::fs::create_dir(&foo).unwrap();
        assert_eq!(validate_addons_dir(&addons), Ok(None));
        assert_eq!(validate_addons_dir(&foo), Ok(Some(DirWarning::NotNamedAddOns)));
        assert_eq!(std::fs::read_dir(&addons).unwrap().count(), 0, "probe file is removed");
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config { addons_dir: PathBuf::from(r"P:\AddOns") };
        config.save(dir.path()).unwrap();
        assert_eq!(Config::load(dir.path()), Some(config));
    }
}
