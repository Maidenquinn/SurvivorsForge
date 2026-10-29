//! Pure view logic: turns state, registry and update results into rows for the UI.

use crate::archive::DetectedAddon;
use crate::registry::Registry;
use crate::scan::InstalledAddon;
use crate::state::{Reconciled, Source, State, TOC_PREFIX};
use crate::updates::UpdateStatus;
use forge_core::trust::Trust;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Badge {
    Verified,
    GitHub,
    Zip,
    Unmanaged,
}

impl Badge {
    pub fn label(self) -> &'static str {
        match self {
            Badge::Verified => "Verified",
            Badge::GitHub => "GitHub",
            Badge::Zip => "Zip",
            Badge::Unmanaged => "Unmanaged",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowState {
    Unchecked,
    UpToDate,
    UpdateAvailable(String),
    Manual,
    CheckFailed(String),
    Missing,
    Unmanaged,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Entry id, or `folder:<name>` for an unmanaged folder.
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub badge: Badge,
    pub state: RowState,
    pub description: String,
    pub repo: Option<String>,
    pub out_of_date: bool,
    pub folders: Vec<String>,
}

pub const UNMANAGED_PREFIX: &str = "folder:";

/// `toc:1.0` -> `1.0`, `zip:file.zip` -> `file.zip`, a 40-char SHA -> its first 7 characters.
pub fn display_version(version: &str) -> String {
    if let Some(v) = version.strip_prefix(TOC_PREFIX) {
        return if v.is_empty() { "unknown".into() } else { v.into() };
    }
    if let Some(file) = version.strip_prefix("zip:") {
        return file.into();
    }
    // A bare SHA or a tag ending in one (`build-<sha>`) keeps 7 characters of the SHA.
    let sha_start = version.len().saturating_sub(40);
    let tail = &version[sha_start..];
    if tail.len() == 40 && tail.chars().all(|c| c.is_ascii_hexdigit()) && (sha_start == 0 || version[..sha_start].ends_with('-')) {
        return format!("{}{}", &version[..sha_start], &tail[..7]);
    }
    version.into()
}

pub fn installed_rows(
    state: &State,
    reconciled: &Reconciled,
    scan: &[InstalledAddon],
    registry: &Registry,
    statuses: &HashMap<String, UpdateStatus>,
) -> Vec<Row> {
    let mut rows: Vec<Row> = state
        .entries
        .iter()
        .map(|entry| {
            let toc = entry
                .folders
                .iter()
                .find_map(|f| scan.iter().find(|s| s.folder.eq_ignore_ascii_case(f)))
                .map(|s| &s.toc);
            let (badge, repo, author, description) = match &entry.source {
                Source::Verified { registry_id } => {
                    let addon = registry.get(registry_id);
                    (
                        Badge::Verified,
                        addon.map(|a| a.repo.clone()),
                        addon.map(|a| a.author.clone()).unwrap_or_default(),
                        addon.map(|a| a.description.clone()).unwrap_or_default(),
                    )
                }
                Source::GitHub { repo, .. } => (
                    Badge::GitHub,
                    Some(repo.clone()),
                    repo.split('/').next().unwrap_or_default().to_string(),
                    toc.and_then(|t| t.notes.clone()).unwrap_or_default(),
                ),
                Source::Zip { .. } => (
                    Badge::Zip,
                    None,
                    toc.and_then(|t| t.author.clone()).unwrap_or_default(),
                    toc.and_then(|t| t.notes.clone()).unwrap_or_default(),
                ),
            };
            let state = if reconciled.missing.contains(&entry.id) {
                RowState::Missing
            } else {
                match statuses.get(&entry.id) {
                    None => RowState::Unchecked,
                    Some(UpdateStatus::UpToDate) => RowState::UpToDate,
                    Some(UpdateStatus::UpdateAvailable(r)) => RowState::UpdateAvailable(r.label.clone()),
                    Some(UpdateStatus::Manual) => RowState::Manual,
                    Some(UpdateStatus::CheckFailed(m)) => RowState::CheckFailed(m.clone()),
                }
            };
            Row {
                id: entry.id.clone(),
                name: entry.name.clone(),
                author,
                version: display_version(&entry.version),
                badge,
                state,
                description,
                repo,
                out_of_date: toc.is_some_and(|t| t.is_out_of_date()),
                folders: entry.folders.clone(),
            }
        })
        .collect();
    rows.extend(reconciled.unmanaged.iter().map(|addon| Row {
        id: format!("{UNMANAGED_PREFIX}{}", addon.folder),
        name: addon.toc.title.clone(),
        author: addon.toc.author.clone().unwrap_or_default(),
        version: addon.toc.version.clone().unwrap_or_default(),
        badge: Badge::Unmanaged,
        state: RowState::Unmanaged,
        description: addon.toc.notes.clone().unwrap_or_default(),
        repo: None,
        out_of_date: addon.toc.is_out_of_date(),
        folders: vec![addon.folder.clone()],
    }));
    rows.sort_by_key(|r| r.name.to_lowercase());
    rows
}

/// The label of a platform trust level, the same words the website uses.
pub fn trust_label(trust: Trust, verified_author: bool) -> &'static str {
    match trust {
        Trust::Verified => "Verified",
        Trust::Scanned if verified_author => "Verified author",
        Trust::Scanned => "Checked",
        Trust::Flagged => "Flagged",
        Trust::Unscanned => "Not checked",
        Trust::Blocked => "Blocked",
        Trust::SourceChanged => "File changed",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrowseRow {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub category: String,
    pub repo: String,
    pub installed: bool,
    /// The platform's trust label of the newest version; `None` from the plain verified list.
    pub trust: Option<&'static str>,
    /// False when the platform blocks the newest version.
    pub installable: bool,
}

pub fn browse_rows(registry: &Registry, state: &State) -> Vec<BrowseRow> {
    registry
        .addons
        .iter()
        .map(|a| BrowseRow {
            id: a.id.clone(),
            name: a.name.clone(),
            author: a.author.clone(),
            description: a.description.clone(),
            category: a.category.clone(),
            repo: a.repo.clone(),
            // Also counts as installed when its folders came from a zip or another repo.
            installed: state.get(&a.id).is_some() || a.folders.iter().any(|f| state.owner_of(f).is_some()),
            trust: a.latest.as_ref().map(|l| trust_label(l.trust, a.verified_author == Some(true))),
            installable: a.latest.as_ref().is_none_or(|l| l.trust.installable()),
        })
        .collect()
}

pub fn matches(query: &str, name: &str, author: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || name.to_lowercase().contains(&query) || author.to_lowercase().contains(&query)
}

pub fn update_all_ids(statuses: &HashMap<String, UpdateStatus>) -> Vec<String> {
    let mut ids: Vec<String> = statuses
        .iter()
        .filter(|(_, s)| matches!(s, UpdateStatus::UpdateAvailable(_)))
        .map(|(id, _)| id.clone())
        .collect();
    ids.sort();
    ids
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZipPlan {
    /// Entry id to install under: the existing zip entry on an update, a new one otherwise.
    pub id: String,
    pub name: String,
    pub addons: Vec<String>,
    /// True when this replaces an existing zip install of the same addon.
    pub is_update: bool,
    /// Entries from GitHub or the verified list that lose their folders to this zip.
    pub takes_over: Vec<(String, String)>,
    /// Folders on disk that SurvivorsForge does not manage yet and that get overwritten.
    pub overwrites_unmanaged: Vec<String>,
}

pub fn plan_zip_install(detected: &[DetectedAddon], state: &State, scan: &[InstalledAddon]) -> ZipPlan {
    let mut owners: Vec<(String, String, bool)> = Vec::new();
    let mut overwrites_unmanaged = Vec::new();
    for addon in detected {
        match state.owner_of(&addon.name) {
            Some(owner) if !owners.iter().any(|o| o.0 == owner.id) => {
                owners.push((owner.id.clone(), owner.name.clone(), matches!(owner.source, Source::Zip { .. })));
            }
            Some(_) => {}
            None if scan.iter().any(|s| s.folder.eq_ignore_ascii_case(&addon.name)) => {
                overwrites_unmanaged.push(addon.name.clone());
            }
            None => {}
        }
    }
    let zip_owner = match owners.as_slice() {
        [(id, name, true)] => Some((id.clone(), name.clone())),
        _ => None,
    };
    let first = &detected[0];
    let (id, name) = zip_owner
        .clone()
        .unwrap_or_else(|| (format!("zip-{}", first.name.to_lowercase()), first.toc.title.clone()));
    ZipPlan {
        id,
        name,
        addons: detected.iter().map(|a| a.name.clone()).collect(),
        is_update: zip_owner.is_some(),
        takes_over: if zip_owner.is_some() {
            vec![]
        } else {
            owners.into_iter().map(|(id, name, _)| (id, name)).collect()
        },
        overwrites_unmanaged,
    }
}

impl ZipPlan {
    pub fn describe(&self) -> String {
        let mut text = format!(
            "{} {}: {}.",
            if self.is_update { "Update" } else { "Install" },
            self.name,
            self.addons.join(", ")
        );
        if !self.takes_over.is_empty() {
            let names: Vec<&str> = self.takes_over.iter().map(|(_, n)| n.as_str()).collect();
            text += &format!(
                "\n\nThis replaces {}. Its source switches to this zip and automatic updates stop.",
                names.join(", ")
            );
        }
        if !self.overwrites_unmanaged.is_empty() {
            text += &format!("\n\nExisting folders get replaced: {}.", self.overwrites_unmanaged.join(", "));
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Entry;
    use crate::toc::Toc;
    use crate::updates::RemoteVersion;

    fn scanned(folder: &str) -> InstalledAddon {
        InstalledAddon { folder: folder.into(), toc: Toc { title: folder.into(), ..Toc::default() } }
    }

    fn detected(name: &str) -> DetectedAddon {
        DetectedAddon { name: name.into(), prefix: name.into(), toc: Toc { title: format!("{name} Title"), ..Toc::default() } }
    }

    fn entry(id: &str, folder: &str, source: Source) -> Entry {
        Entry { id: id.into(), name: id.into(), source, folders: vec![folder.into()], version: "v1".into(), installed_at: 0 }
    }

    fn zip_source() -> Source {
        Source::Zip { file_name: "a.zip".into() }
    }

    #[test]
    fn filter_matches_name_and_author_case_insensitive() {
        assert!(matches("lucky", "LuckyOdds", "cadoky"));
        assert!(matches("CADO", "LuckyOdds", "cadoky"));
        assert!(matches("  ", "x", "y"));
        assert!(!matches("bordo", "LuckyOdds", "cadoky"));
    }

    #[test]
    fn rows_from_state_have_correct_badges() {
        let mut state = State::default();
        state.upsert(entry("luckyodds", "LuckyOdds", Source::Verified { registry_id: "luckyodds".into() }));
        state.upsert(entry("gh:o/r", "R", Source::GitHub { repo: "o/r".into(), spec: crate::registry::SourceSpec::Tag }));
        state.upsert(entry("zip-z", "Z", zip_source()));
        let scan = [scanned("LuckyOdds"), scanned("R"), scanned("Z"), scanned("Tickfire")];
        let reconciled = state.reconcile(&scan);
        let mut statuses = HashMap::new();
        statuses.insert("luckyodds".to_string(), UpdateStatus::UpdateAvailable(RemoteVersion {
            id: "v0.6.0".into(),
            label: "v0.6.0".into(),
            download: String::new(),
        }));
        let rows = installed_rows(&state, &reconciled, &scan, &crate::registry::embedded(), &statuses);
        let summary: Vec<(&str, Badge)> = rows.iter().map(|r| (r.id.as_str(), r.badge)).collect();
        assert_eq!(summary, [
            ("gh:o/r", Badge::GitHub),
            ("luckyodds", Badge::Verified),
            ("folder:Tickfire", Badge::Unmanaged),
            ("zip-z", Badge::Zip),
        ]);
        assert_eq!(rows[1].state, RowState::UpdateAvailable("v0.6.0".into()));
        assert_eq!(rows[1].author, "cadoky");
        assert_eq!(rows[0].state, RowState::Unchecked);
    }

    #[test]
    fn missing_folder_shows_missing() {
        let mut state = State::default();
        state.upsert(entry("zip-z", "Z", zip_source()));
        let rows = installed_rows(&state, &state.reconcile(&[]), &[], &crate::registry::embedded(), &HashMap::new());
        assert_eq!(rows[0].state, RowState::Missing);
    }

    #[test]
    fn update_all_selects_only_update_available() {
        let remote = RemoteVersion { id: "v2".into(), label: "v2".into(), download: String::new() };
        let statuses = HashMap::from([
            ("b".to_string(), UpdateStatus::UpdateAvailable(remote.clone())),
            ("a".to_string(), UpdateStatus::UpdateAvailable(remote)),
            ("c".to_string(), UpdateStatus::UpToDate),
            ("d".to_string(), UpdateStatus::CheckFailed("x".into())),
        ]);
        assert_eq!(update_all_ids(&statuses), ["a", "b"]);
    }

    #[test]
    fn display_versions() {
        assert_eq!(display_version("toc:0.4.0"), "0.4.0");
        assert_eq!(display_version("toc:"), "unknown");
        assert_eq!(display_version("zip:a.zip"), "a.zip");
        assert_eq!(display_version("2030b8c58c6510b91c9835b02d22b416e7329e39"), "2030b8c");
        assert_eq!(display_version("build-3c7ce887d15aef46d8e0c17b9fe71142cb37c4ba"), "build-3c7ce88");
        assert_eq!(display_version("v1.0"), "v1.0");
    }

    #[test]
    fn zip_plan_new_addon() {
        let plan = plan_zip_install(&[detected("Tickfire")], &State::default(), &[]);
        assert_eq!(plan.id, "zip-tickfire");
        assert_eq!(plan.name, "Tickfire Title");
        assert!(!plan.is_update && plan.takes_over.is_empty() && plan.overwrites_unmanaged.is_empty());
    }

    #[test]
    fn zip_plan_updates_existing_zip_entry() {
        let mut state = State::default();
        state.upsert(entry("zip-a", "A", zip_source()));
        let plan = plan_zip_install(&[detected("A")], &state, &[scanned("A")]);
        assert_eq!(plan.id, "zip-a");
        assert!(plan.is_update);
        assert!(plan.takes_over.is_empty());
    }

    #[test]
    fn zip_plan_requires_confirmation_for_managed_source() {
        let mut state = State::default();
        state.upsert(entry("luckyodds", "LuckyOdds", Source::Verified { registry_id: "luckyodds".into() }));
        let plan = plan_zip_install(&[detected("LuckyOdds")], &state, &[scanned("LuckyOdds")]);
        assert_eq!(plan.takes_over, [("luckyodds".to_string(), "luckyodds".to_string())]);
        assert!(!plan.is_update);
        assert!(plan.describe().contains("automatic updates stop"));
    }

    #[test]
    fn zip_plan_reports_unmanaged_overwrite() {
        let plan = plan_zip_install(&[detected("Tickfire")], &State::default(), &[scanned("Tickfire")]);
        assert_eq!(plan.overwrites_unmanaged, ["Tickfire"]);
    }

    fn platform_registry(trust: Trust) -> Registry {
        let mut registry = crate::registry::embedded();
        for addon in &mut registry.addons {
            addon.latest = Some(crate::registry::LatestVersion {
                tag: "v1".into(),
                asset_url: "https://github.com/a.zip".into(),
                sha256: None,
                size: None,
                trust,
                findings: Default::default(),
                game_min: None,
                game_max: None,
            });
        }
        registry
    }

    #[test]
    fn badge_per_trust() {
        let cases = [
            (Trust::Verified, false, "Verified"),
            (Trust::Scanned, true, "Verified author"),
            (Trust::Scanned, false, "Checked"),
            (Trust::Flagged, false, "Flagged"),
            (Trust::Unscanned, false, "Not checked"),
            (Trust::Blocked, false, "Blocked"),
            (Trust::SourceChanged, false, "File changed"),
        ];
        for (trust, author, label) in cases {
            assert_eq!(trust_label(trust, author), label);
        }
        let rows = browse_rows(&platform_registry(Trust::Flagged), &State::default());
        assert!(rows.iter().all(|r| r.trust == Some("Flagged")));
        let plain = browse_rows(&crate::registry::embedded(), &State::default());
        assert!(plain.iter().all(|r| r.trust.is_none() && r.installable), "the plain verified list has no trust data");
    }

    #[test]
    fn blocked_rows_not_installable() {
        for trust in [Trust::Blocked, Trust::SourceChanged] {
            let rows = browse_rows(&platform_registry(trust), &State::default());
            assert!(rows.iter().all(|r| !r.installable));
        }
        assert!(browse_rows(&platform_registry(Trust::Flagged), &State::default()).iter().all(|r| r.installable));
    }
}
