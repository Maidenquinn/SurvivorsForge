//! Finds the addons installed in an AddOns folder.

use crate::toc::Toc;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct InstalledAddon {
    pub folder: String,
    pub toc: Toc,
}

/// An addon is a folder `<Name>/` holding `<Name>.toc` (the WoW 1.12 rule).
/// Files (like `account.txt`) and hidden folders are skipped and never read.
pub fn scan_addons(addons_dir: &Path) -> std::io::Result<Vec<InstalledAddon>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(addons_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        if folder.starts_with('.') {
            continue;
        }
        if let Ok(bytes) = std::fs::read(entry.path().join(format!("{folder}.toc"))) {
            found.push(InstalledAddon { toc: Toc::parse_bytes(&bytes, &folder), folder });
        }
    }
    found.sort_by_key(|a| a.folder.to_lowercase());
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scan_lists_only_folders_with_matching_toc() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join("A")).unwrap();
        fs::write(p.join("A/A.toc"), "## Version: 1.0").unwrap();
        fs::create_dir_all(p.join("B")).unwrap();
        fs::write(p.join("B/other.toc"), "").unwrap();
        fs::write(p.join("account.txt"), "secret").unwrap();
        let found = scan_addons(p).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].folder, "A");
        assert_eq!(found[0].toc.version.as_deref(), Some("1.0"));
    }
}
