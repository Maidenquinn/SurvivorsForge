//! Trust levels of an addon version, as the API reports them. The API decides them in one place
//! (the `version_trust` view); clients only read them.

use serde::{Deserialize, Serialize};

/// Highest priority first: a blocked version stays blocked whatever else is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    /// An admin blocked the version or the whole addon. Not installable.
    Blocked,
    /// The file on GitHub differs from the scanned one. Not installable.
    SourceChanged,
    /// An admin approved exactly this file.
    Verified,
    /// The automatic scan found something.
    Flagged,
    /// The automatic scan found nothing; no admin looked at it.
    Scanned,
    /// Not scanned yet, or the scan failed.
    Unscanned,
}

impl Trust {
    /// Blocked and changed files are never installed.
    pub fn installable(self) -> bool {
        !matches!(self, Trust::Blocked | Trust::SourceChanged)
    }

    /// Installing asks the user first.
    pub fn needs_confirmation(self) -> bool {
        matches!(self, Trust::Flagged | Trust::Unscanned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_api() {
        let all = [Trust::Blocked, Trust::SourceChanged, Trust::Verified, Trust::Flagged, Trust::Scanned, Trust::Unscanned];
        let names: Vec<String> = all.iter().map(|t| serde_json::to_value(t).unwrap().as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["blocked", "source_changed", "verified", "flagged", "scanned", "unscanned"]);
        assert!(!Trust::Blocked.installable() && !Trust::SourceChanged.installable() && Trust::Flagged.installable());
        assert!(Trust::Flagged.needs_confirmation() && Trust::Unscanned.needs_confirmation() && !Trust::Scanned.needs_confirmation());
    }
}
