//! What the website says about each trust level. One place, so list badges, addon pages and the
//! download confirmation never disagree.

/// The notice shown with a version. `class` is one of the CSS notice classes.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub class: &'static str,
    pub badge: &'static str,
    pub text: &'static str,
    /// A download link exists at all.
    pub downloadable: bool,
    /// The download goes through the confirmation page first.
    pub confirm: bool,
}

pub fn notice(trust: &str, verified_author: bool, source_missing: bool) -> Notice {
    if source_missing && !matches!(trust, "blocked" | "source_changed") {
        return Notice {
            class: "",
            badge: "Unavailable",
            text: "The file is no longer available on GitHub.",
            downloadable: false,
            confirm: false,
        };
    }
    match trust {
        "verified" => Notice { class: "ok", badge: "Verified", text: "Verified by an admin.", downloadable: true, confirm: false },
        "scanned" if verified_author => Notice {
            class: "",
            badge: "Verified author",
            text: "Verified author, this version was checked automatically.",
            downloadable: true,
            confirm: false,
        },
        "scanned" => Notice {
            class: "",
            badge: "Checked",
            text: "Checked automatically, nothing found. Not verified by hand.",
            downloadable: true,
            confirm: false,
        },
        "flagged" => Notice {
            class: "bad",
            badge: "Flagged",
            text: "The automatic check found something suspicious. Read the findings before you install it.",
            downloadable: true,
            confirm: true,
        },
        "blocked" => Notice { class: "bad", badge: "Blocked", text: "An admin blocked this version. It cannot be downloaded.", downloadable: false, confirm: false },
        "source_changed" => Notice {
            class: "bad",
            badge: "File changed",
            text: "The file on GitHub was changed after it was checked. It cannot be downloaded until an admin looks at it.",
            downloadable: false,
            confirm: false,
        },
        _ => Notice {
            class: "warn",
            badge: "Not checked",
            text: "Not checked automatically yet. Install it at your own risk.",
            downloadable: true,
            confirm: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert!(notice("verified", false, false).downloadable && !notice("verified", false, false).confirm);
        assert_eq!(notice("scanned", true, false).badge, "Verified author");
        assert!(notice("flagged", false, false).confirm);
        assert!(notice("unscanned", false, false).confirm);
        assert!(!notice("blocked", false, false).downloadable);
        assert!(!notice("source_changed", false, false).downloadable);
        assert!(!notice("scanned", false, true).downloadable, "missing on GitHub");
        assert_eq!(notice("blocked", false, true).badge, "Blocked", "blocked wins over missing");
    }
}
