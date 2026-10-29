//! Static security scanner for addon zips. Pure functions on bytes: no files, no network.
//!
//! A clean report means "nothing found", never "safe".

pub mod finding;
pub mod zip_rules;

pub use finding::{Finding, Report, Severity};

/// The version of the rules. A new version lets the platform rescan everything.
pub const SCANNER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Scans `zip` for an addon that registered `expected_folders`. `foreign_folders` belong to other addons.
pub fn scan(zip: &[u8], expected_folders: &[String], foreign_folders: &[String]) -> Report {
    let mut report = Report::default();
    zip_rules::check(zip, expected_folders, foreign_folders, &mut report);
    report
}
