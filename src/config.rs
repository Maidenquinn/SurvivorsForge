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

/// Where the game is usually unpacked: the home folder and its Games, Downloads and Desktop
/// folders, removable media on Linux (Steam Deck SD card), every drive root on Windows.
pub fn search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(user) = directories::UserDirs::new() {
        roots.push(user.home_dir().join("Games"));
        roots.extend(user.download_dir().map(Path::to_path_buf));
        roots.extend(user.desktop_dir().map(Path::to_path_buf));
        roots.push(user.home_dir().to_path_buf());
    }
    if cfg!(windows) {
        roots.extend((b'C'..=b'Z').map(|d| PathBuf::from(format!("{}:\\", d as char))).filter(|p| p.is_dir()));
    } else {
        // `/run/media/<user>/<label>` on most distros, `/run/media/<device>` on SteamOS
        roots.push(PathBuf::from("/run/media"));
    }
    roots.into_iter().filter(|p| p.is_dir()).collect()
}

/// Folders that look like the game's AddOns folder (`<game>/benilla-config/AddOns`), found by a
/// short walk below each root. Hidden folders and symlinks are skipped, and each root has a budget
/// of directory entries, so a huge drive can't stall the setup.
pub fn find_addons_dirs(roots: &[PathBuf], max_depth: usize) -> Vec<PathBuf> {
    const BUDGET_PER_ROOT: usize = 20_000;
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let mut budget = BUDGET_PER_ROOT;
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let candidate = dir.join("benilla-config").join("AddOns");
            if candidate.is_dir() && !found.contains(&candidate) {
                found.push(candidate);
            }
            if depth >= max_depth {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                if budget == 0 {
                    break;
                }
                budget -= 1;
                let hidden = entry.file_name().to_string_lossy().starts_with(['.', '$']);
                if !hidden && entry.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push((entry.path(), depth + 1));
                }
            }
        }
    }
    found.sort();
    found
}

/// Steam Deck Game Mode (and any gamescope session): the app runs fullscreen there.
pub fn is_game_mode(xdg_current_desktop: Option<&str>, gamescope_display: bool) -> bool {
    gamescope_display || xdg_current_desktop.is_some_and(|d| d.eq_ignore_ascii_case("gamescope"))
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
    fn suggests_existing_addon_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let game = root.join("Games/AzerothSurvivors/benilla-config/AddOns");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(root.join(".hidden/AzerothSurvivors/benilla-config/AddOns")).unwrap();
        std::fs::create_dir_all(root.join("a/b/c/d/benilla-config/AddOns")).unwrap();
        assert_eq!(find_addons_dirs(&[root.to_path_buf()], 3), std::slice::from_ref(&game));
        assert_eq!(find_addons_dirs(&[root.to_path_buf(), root.join("Games")], 3), [game], "no duplicates");
    }

    #[test]
    fn detects_game_mode_from_env() {
        assert!(is_game_mode(Some("gamescope"), false));
        assert!(is_game_mode(None, true));
        assert!(!is_game_mode(Some("KDE"), false));
        assert!(!is_game_mode(None, false));
    }

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
