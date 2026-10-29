//! Rules for the zip itself: paths, file types, sizes and addon folders. Works on bytes in memory.

use crate::finding::{Report, Severity};
use forge_core::archive::{analyze_reader, safe_components};
use std::io::{Cursor, Read};

pub const MAX_ENTRIES: usize = 5000;
pub const MAX_ENTRY_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 200 * 1024 * 1024;
pub const MAX_RATIO: u64 = 100;
/// The ratio rule only applies from this size on: small text files compress well and are harmless.
pub const RATIO_MIN_BYTES: u64 = 1024 * 1024;
pub const MAX_DEPTH: usize = 10;

pub const ALLOWED_EXTENSIONS: &[&str] =
    &["lua", "toc", "xml", "tga", "blp", "png", "jpg", "jpeg", "ttf", "otf", "mp3", "ogg", "wav", "txt", "md"];
const LICENSE_NAMES: &[&str] = &["license", "licence", "copying", "readme", "changelog"];
/// Files whose content later rules read. Everything else is only sniffed.
const KEPT_EXTENSIONS: &[&str] = &["lua", "toc", "xml"];

/// A file of the zip whose content the content rules need.
pub struct TextFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase())
}

fn is_hidden(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("__macosx/")
        || lower.split('/').any(|p| p == ".git")
        || lower.ends_with(".ds_store")
        || lower.ends_with("thumbs.db")
}

/// What the first bytes say a file is, if it is something an addon must never contain.
pub fn sniff(head: &[u8]) -> Option<(&'static str, &'static str)> {
    const EXECUTABLES: &[(&[u8], &str)] = &[
        (b"MZ", "a Windows program"),
        (b"\x7fELF", "a Linux program"),
        (b"\xfe\xed\xfa\xce", "a macOS program"),
        (b"\xfe\xed\xfa\xcf", "a macOS program"),
        (b"\xce\xfa\xed\xfe", "a macOS program"),
        (b"\xcf\xfa\xed\xfe", "a macOS program"),
        (b"\xca\xfe\xba\xbe", "a macOS program"),
        (b"#!", "a script"),
    ];
    const ARCHIVES: &[(&[u8], &str)] = &[
        (b"PK\x03\x04", "a zip archive"),
        (b"PK\x05\x06", "a zip archive"),
        (b"Rar!", "a RAR archive"),
        (b"7z\xbc\xaf\x27\x1c", "a 7z archive"),
        (b"\x1f\x8b", "a gzip archive"),
        (b"BZh", "a bzip2 archive"),
        (b"\xfd7zXZ\x00", "an xz archive"),
    ];
    if let Some((_, what)) = EXECUTABLES.iter().find(|(magic, _)| head.starts_with(magic)) {
        return Some(("zip.executable", what));
    }
    if let Some((_, what)) = ARCHIVES.iter().find(|(magic, _)| head.starts_with(magic)) {
        return Some(("zip.nested_archive", what));
    }
    if head.len() >= 262 && &head[257..262] == b"ustar" {
        return Some(("zip.nested_archive", "a tar archive"));
    }
    None
}

/// Runs the zip rules and returns the `.lua`, `.toc` and `.xml` files for the content rules.
pub fn check(zip: &[u8], expected_folders: &[String], foreign_folders: &[String], report: &mut Report) -> Vec<TextFile> {
    let mut archive = match zip::ZipArchive::new(Cursor::new(zip)) {
        Ok(archive) => archive,
        Err(e) => {
            report.add("zip.unreadable", Severity::Critical, "", None, format!("The zip cannot be opened: {e}"));
            return Vec::new();
        }
    };
    if archive.len() > MAX_ENTRIES {
        report.add("zip.bomb", Severity::Critical, "", None, format!("The zip has {} entries, more than {MAX_ENTRIES}", archive.len()));
        return Vec::new();
    }

    let mut texts = Vec::new();
    let mut total: u64 = 0;
    let mut unsafe_paths = false;
    let mut deep_reported = false;
    for i in 0..archive.len() {
        let (path, is_dir, encrypted, mode, compressed) = match archive.by_index_raw(i) {
            Ok(entry) => (
                String::from_utf8_lossy(entry.name_raw()).into_owned(),
                entry.is_dir(),
                entry.encrypted(),
                entry.unix_mode(),
                entry.compressed_size(),
            ),
            Err(e) => {
                report.add("zip.unreadable", Severity::Critical, "", None, format!("Entry {i} cannot be read: {e}"));
                continue;
            }
        };
        if path.contains('\0') || path.contains('\\') || safe_components(&path).is_err() {
            report.add("zip.path_traversal", Severity::Critical, &path, None, "The path could write outside the addon folder");
            unsafe_paths = true;
            continue;
        }
        if mode.is_some_and(|m| m & 0o170000 == 0o120000) {
            report.add("zip.symlink", Severity::Critical, &path, None, "Symbolic links are not allowed");
            continue;
        }
        if encrypted {
            report.add("zip.unreadable", Severity::Critical, &path, None, "The entry is encrypted and cannot be checked");
            continue;
        }
        if is_dir {
            continue;
        }
        let depth = path.trim_end_matches('/').split('/').count().saturating_sub(1);
        if depth > MAX_DEPTH && !deep_reported {
            report.add("zip.deep_nesting", Severity::Warn, &path, None, format!("More than {MAX_DEPTH} folder levels"));
            deep_reported = true;
        }

        let ext = extension(&path);
        let keep = ext.as_deref().is_some_and(|e| KEPT_EXTENSIONS.contains(&e));
        let mut entry = match archive.by_index(i) {
            Ok(entry) => entry,
            Err(e) => {
                report.add("zip.unreadable", Severity::Critical, &path, None, format!("The entry cannot be read: {e}"));
                continue;
            }
        };
        // Count what really comes out, never trust the sizes in the header.
        let mut head = Vec::with_capacity(512);
        let mut kept = Vec::new();
        let mut size: u64 = 0;
        let mut buf = [0u8; 64 * 1024];
        let mut too_big = false;
        loop {
            let n = match entry.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    report.add("zip.unreadable", Severity::Critical, &path, None, format!("The entry cannot be read: {e}"));
                    break;
                }
            };
            if head.len() < 512 {
                head.extend_from_slice(&buf[..n.min(512 - head.len())]);
            }
            size += n as u64;
            if size > MAX_ENTRY_BYTES || total + size > MAX_TOTAL_BYTES {
                too_big = true;
                break;
            }
            if keep {
                kept.extend_from_slice(&buf[..n]);
            }
        }
        total += size;
        if too_big {
            let message = if size > MAX_ENTRY_BYTES {
                format!("A single file larger than {} MB", MAX_ENTRY_BYTES / 1024 / 1024)
            } else {
                format!("The files add up to more than {} MB", MAX_TOTAL_BYTES / 1024 / 1024)
            };
            report.add("zip.bomb", Severity::Critical, &path, None, message);
            if total > MAX_TOTAL_BYTES {
                return texts;
            }
            continue;
        }
        if size >= RATIO_MIN_BYTES && size / compressed.max(1) > MAX_RATIO {
            report.add("zip.bomb", Severity::Critical, &path, None, format!("Compressed more than {MAX_RATIO} to 1"));
        }
        if let Some((rule, what)) = sniff(&head) {
            report.add(rule, Severity::Critical, &path, None, format!("The file is {what}"));
            continue;
        }
        if is_hidden(&path) {
            report.add("zip.hidden", Severity::Info, &path, None, "System or version control file");
            continue;
        }
        let stem = path.rsplit('/').next().unwrap_or(&path).to_ascii_lowercase();
        let allowed = match &ext {
            Some(ext) => ALLOWED_EXTENSIONS.contains(&ext.as_str()),
            None => LICENSE_NAMES.contains(&stem.as_str()),
        };
        if !allowed {
            report.add("zip.extension", Severity::Warn, &path, None, "This file type does not belong in an addon");
        }
        if keep {
            texts.push(TextFile { path, bytes: kept });
        }
    }
    if !unsafe_paths {
        check_folders(zip, expected_folders, foreign_folders, &texts, report);
    }
    texts
}

/// The addon folders in the zip against the folders the addon registered.
fn check_folders(zip: &[u8], expected: &[String], foreign: &[String], texts: &[TextFile], report: &mut Report) {
    let detected = analyze_reader(Cursor::new(zip)).unwrap_or_default();
    for addon in &detected {
        if foreign.iter().any(|f| f.eq_ignore_ascii_case(&addon.name)) {
            report.add("zip.foreign_folder", Severity::Critical, &addon.prefix, None,
                format!("The folder {} belongs to another addon and would overwrite it", addon.name));
        } else if !expected.iter().any(|e| e.eq_ignore_ascii_case(&addon.name)) {
            report.add("zip.undeclared_folder", Severity::Critical, &addon.prefix, None,
                format!("The folder {} is not registered for this addon", addon.name));
        }
        if addon.toc.interface.as_deref().map(str::trim) != Some(forge_core::toc::INTERFACE) {
            report.add("zip.toc_interface", Severity::Info, &addon.prefix, None,
                format!("The .toc is not for interface {}", forge_core::toc::INTERFACE));
        }
        check_toc_files(addon, texts, report);
    }
    for folder in expected {
        if !detected.iter().any(|a| a.name.eq_ignore_ascii_case(folder)) {
            report.add("zip.missing_folder", Severity::Warn, "", None, format!("The registered folder {folder} is not in the zip"));
        }
    }
}

/// Every file the `.toc` loads exists in the addon folder.
fn check_toc_files(addon: &forge_core::archive::DetectedAddon, texts: &[TextFile], report: &mut Report) {
    let dir = if addon.prefix.is_empty() { String::new() } else { format!("{}/", addon.prefix) };
    let toc = texts.iter().find(|t| {
        t.path.len() > dir.len()
            && t.path[..dir.len()].eq_ignore_ascii_case(&dir)
            && t.path[dir.len()..].eq_ignore_ascii_case(&format!("{}.toc", addon.name))
    });
    let Some(toc) = toc else { return };
    let all: Vec<String> = texts.iter().map(|t| t.path.to_ascii_lowercase()).collect();
    for (n, line) in String::from_utf8_lossy(&toc.bytes).lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let wanted = format!("{dir}{}", line.replace('\\', "/")).to_ascii_lowercase();
        if !all.contains(&wanted) {
            report.add_with_snippet("zip.toc_missing_file", Severity::Warn, &toc.path, Some(n as u32 + 1),
                "The .toc loads a file that is not in the zip", line);
        }
    }
}
