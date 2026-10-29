//! Parser for WoW `.toc` files (`## Key: Value` header lines).

/// The interface version of the Azeroth Survivors client (WoW 1.12.1).
pub const INTERFACE: &str = "11200";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Toc {
    pub title: String,
    pub version: Option<String>,
    pub interface: Option<String>,
    pub notes: Option<String>,
    pub author: Option<String>,
}

impl Toc {
    pub fn parse(text: &str, folder: &str) -> Toc {
        let mut toc = Toc::default();
        let mut title = None;
        for line in text.trim_start_matches('\u{feff}').lines() {
            let Some((key, value)) = line.strip_prefix("##").and_then(|rest| rest.split_once(':')) else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            let value = Some(value.to_string());
            match key.trim().to_ascii_lowercase().as_str() {
                "title" => title = value,
                "version" => toc.version = value,
                "interface" => toc.interface = value,
                "notes" => toc.notes = value.map(|v| strip_colors(&v)),
                "author" => toc.author = value.map(|v| strip_colors(&v)),
                _ => {}
            }
        }
        toc.title = title
            .map(|t| strip_colors(&t))
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| folder.to_string());
        toc
    }

    /// Reads bytes leniently: some addons ship Latin-1 `.toc` files.
    pub fn parse_bytes(bytes: &[u8], folder: &str) -> Toc {
        Toc::parse(&String::from_utf8_lossy(bytes), folder)
    }

    pub fn is_out_of_date(&self) -> bool {
        self.interface.as_deref().is_some_and(|i| i.trim() != INTERFACE)
    }
}

/// Removes WoW color escapes: `|cAARRGGBB` and `|r`. `||` is a literal pipe.
pub fn strip_colors(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '|' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('c' | 'C') => {
                chars.next();
                for _ in 0..8 {
                    chars.next();
                }
            }
            Some('r' | 'R') => {
                chars.next();
            }
            Some('|') => {
                chars.next();
                out.push('|');
            }
            _ => out.push('|'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fields_case_insensitive_with_bom_and_crlf() {
        let text = "\u{feff}## Interface: 11200\r\n## TITLE: Lucky\r\n## version: 0.4.0\r\n## Author: cadoky\r\nLuckyOdds.lua\r\n";
        let toc = Toc::parse(text, "LuckyOdds");
        assert_eq!(toc.title, "Lucky");
        assert_eq!(toc.version.as_deref(), Some("0.4.0"));
        assert_eq!(toc.interface.as_deref(), Some("11200"));
        assert_eq!(toc.author.as_deref(), Some("cadoky"));
    }

    #[test]
    fn strips_color_codes() {
        let toc = Toc::parse("## Title: |cffa335eeLucky|rOdds", "LuckyOdds");
        assert_eq!(toc.title, "LuckyOdds");
        assert_eq!(strip_colors("|cffff7d0aAzeroth Survivors Boss Mods|r"), "Azeroth Survivors Boss Mods");
        assert_eq!(strip_colors("a||b"), "a|b");
    }

    #[test]
    fn title_falls_back_to_folder() {
        assert_eq!(Toc::parse("## Interface: 11200", "Tickfire").title, "Tickfire");
    }

    #[test]
    fn flags_out_of_date_interface() {
        assert!(!Toc::parse("## Interface: 11200", "A").is_out_of_date());
        assert!(Toc::parse("## Interface: 30300", "A").is_out_of_date());
        assert!(!Toc::parse("", "A").is_out_of_date());
    }
}
