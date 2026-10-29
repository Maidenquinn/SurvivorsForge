//! Static security scanner for addon zips. Pure functions on bytes: no files, no network.
//!
//! A clean report means "nothing found", never "safe".

mod entropy;
pub mod finding;
pub mod lua_rules;
pub mod xml_rules;
pub mod zip_rules;

pub use finding::{Finding, Report, Severity};

/// The version of the rules. A new version lets the platform rescan everything.
pub const SCANNER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Scans `zip` for an addon that registered `expected_folders`. `foreign_folders` belong to other addons.
pub fn scan(zip: &[u8], expected_folders: &[String], foreign_folders: &[String]) -> Report {
    let mut report = Report::default();
    let texts = zip_rules::check(zip, expected_folders, foreign_folders, &mut report);
    let paths: Vec<String> = texts.iter().map(|t| t.path.clone()).collect();
    for text in &texts {
        match text.path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).as_deref() {
            Some("lua") => lua_rules::check(&text.path, &text.bytes, 0, &mut report),
            Some("xml") => xml_rules::check(&text.path, &text.bytes, &paths, &mut report),
            _ => {}
        }
    }
    report
}
