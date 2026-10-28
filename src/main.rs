#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use survivorsforge::config::{self, Config};
use survivorsforge::controller::{self, Badge, Row, RowState, UNMANAGED_PREFIX};
use survivorsforge::github::{normalize_repo, Client, GitHub};
use survivorsforge::install::{self, Request};
use survivorsforge::registry::{self, Origin, Registry, SourceSpec};
use survivorsforge::repo_detect;
use survivorsforge::scan::{scan_addons, InstalledAddon};
use survivorsforge::self_update::{self, Available};
use survivorsforge::state::{Reconciled, Source, State};
use survivorsforge::updates::{self, RemoteVersion, UpdateStatus};
use survivorsforge::{archive, unique_name, Error, APP_REPO, APP_VERSION};

slint::include_modules!();

struct Core {
    data_dir: PathBuf,
    config: Config,
    registry: Registry,
    state: State,
    scan: Vec<InstalledAddon>,
    reconciled: Reconciled,
    statuses: HashMap<String, UpdateStatus>,
    /// Entry ids with an install, update or removal running.
    busy: HashSet<String>,
    checking: bool,
    search: String,
    status: String,
    status_error: bool,
    self_update: Option<Available>,
}

type Shared = Arc<Mutex<Core>>;

fn lock(shared: &Shared) -> MutexGuard<'_, Core> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl Core {
    fn new(data_dir: PathBuf, config: Option<Config>) -> Core {
        let registry = registry::load(Err(Error::Network("not loaded yet".into())), &data_dir.join("cache")).0;
        let mut core = Core {
            data_dir,
            config: Config::default(),
            registry,
            state: State::default(),
            scan: Vec::new(),
            reconciled: Reconciled::default(),
            statuses: HashMap::new(),
            busy: HashSet::new(),
            checking: false,
            search: String::new(),
            status: String::new(),
            status_error: false,
            self_update: None,
        };
        if let Some(config) = config {
            core.switch_config(config);
        }
        core
    }

    /// Applies a (new) config: loads the state of its AddOns folder and scans it.
    fn switch_config(&mut self, config: Config) {
        self.config = config;
        let (state, note) = State::load(&self.data_dir, &self.config.addons_dir);
        self.state = state;
        self.statuses.clear();
        if note.is_some() {
            self.set_status("state.json was damaged and saved as state.json.bak; starting fresh.".into(), true);
        }
        // Leftovers of downloads that were never installed (cancelled dialogs, crashes).
        let _ = std::fs::remove_dir_all(self.downloads());
        self.rescan();
    }

    fn addons_dir(&self) -> &Path {
        &self.config.addons_dir
    }

    fn downloads(&self) -> PathBuf {
        install::work_dir(self.addons_dir()).join("downloads")
    }

    fn client(&self) -> Client {
        Client::new()
    }

    fn rescan(&mut self) {
        if self.addons_dir().as_os_str().is_empty() {
            return;
        }
        match scan_addons(&self.config.addons_dir) {
            Ok(scan) => {
                if self.state.adopt(&scan, &self.registry) {
                    self.save_state();
                }
                self.reconciled = self.state.reconcile(&scan);
                self.scan = scan;
            }
            Err(e) => self.set_status(format!("Cannot read the AddOns folder: {e}"), true),
        }
    }

    fn save_state(&mut self) {
        if let Err(e) = self.state.save(&self.data_dir, &self.config.addons_dir) {
            self.set_status(format!("Could not save state.json: {e}"), true);
        }
    }

    fn set_status(&mut self, text: String, error: bool) {
        self.status = text;
        self.status_error = error;
    }

    fn repo_of(&self, id: &str) -> Option<String> {
        match self.state.get(id).map(|e| &e.source) {
            Some(Source::GitHub { repo, .. }) => Some(repo.clone()),
            Some(Source::Verified { registry_id }) => self.registry.get(registry_id).map(|a| a.repo.clone()),
            Some(Source::Zip { .. }) => None,
            None => self.registry.get(id).map(|a| a.repo.clone()),
        }
    }
}

fn model(rows: Vec<AddonRow>) -> ModelRc<AddonRow> {
    ModelRc::new(VecModel::from(rows))
}

fn initial(name: &str) -> SharedString {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_default().into()
}

fn installed_row(row: &Row, core: &Core) -> AddonRow {
    let (status, kind, action) = match &row.state {
        RowState::Unchecked => (if core.checking { "Checking..." } else { "Not checked" }.to_string(), 0, ""),
        RowState::UpToDate => ("Up to date".into(), 1, ""),
        RowState::UpdateAvailable(label) => (format!("Update: {label}"), 2, "Update"),
        RowState::Manual => ("Installed from zip".into(), 0, ""),
        RowState::CheckFailed(_) => ("Check failed".into(), 3, ""),
        RowState::Missing => ("Folder missing".into(), 3, if row.badge == Badge::Zip { "" } else { "Reinstall" }),
        RowState::Unmanaged => ("Not managed".into(), 0, ""),
    };
    let warning = match &row.state {
        RowState::CheckFailed(message) => message.clone(),
        _ if row.out_of_date => "Interface is not 11200: enable \"Load out of date AddOns\" in the game".into(),
        _ => String::new(),
    };
    AddonRow {
        id: row.id.clone().into(),
        name: row.name.clone().into(),
        initial: initial(&row.name),
        author: row.author.clone().into(),
        version: row.version.clone().into(),
        badge: row.badge.label().into(),
        description: row.description.clone().into(),
        status: status.into(),
        status_kind: kind,
        action: action.into(),
        busy: core.busy.contains(&row.id),
        can_remove: true,
        can_open: row.repo.is_some(),
        warning: warning.into(),
    }
}

fn refresh(ui: &AppWindow, shared: &Shared) {
    let core = lock(shared);
    let rows = controller::installed_rows(&core.state, &core.reconciled, &core.scan, &core.registry, &core.statuses);
    ui.set_installed_total(rows.len() as i32);
    ui.set_installed_rows(model(
        rows.iter()
            .filter(|r| controller::matches(&core.search, &r.name, &r.author))
            .map(|r| installed_row(r, &core))
            .collect(),
    ));
    ui.set_browse_rows(model(
        controller::browse_rows(&core.registry, &core.state)
            .into_iter()
            .filter(|b| controller::matches(&core.search, &b.name, &b.author))
            .map(|b| AddonRow {
                initial: initial(&b.name),
                id: b.id.clone().into(),
                name: b.name.into(),
                author: b.author.into(),
                version: SharedString::new(),
                badge: b.category.into(),
                description: b.description.into(),
                status: if b.installed { "Installed" } else { "" }.into(),
                status_kind: if b.installed { 1 } else { 0 },
                action: if b.installed { "" } else { "Install" }.into(),
                busy: core.busy.contains(&b.id),
                can_remove: false,
                can_open: true,
                warning: SharedString::new(),
            })
            .collect(),
    ));
    ui.set_update_count(controller::update_all_ids(&core.statuses).len() as i32);
    ui.set_checking(core.checking);
    ui.set_status_text(core.status.clone().into());
    ui.set_status_error(core.status_error);
    ui.set_addons_dir(core.config.addons_dir.to_string_lossy().into_owned().into());
    ui.set_self_update_version(core.self_update.as_ref().map(|a| a.version.clone()).unwrap_or_default().into());
}

/// Runs `job` off the UI thread, then shows its result in the status bar and refreshes the UI.
fn run_bg(
    weak: slint::Weak<AppWindow>,
    shared: Shared,
    busy_id: Option<String>,
    job: impl FnOnce(&Shared) -> survivorsforge::Result<String> + Send + 'static,
) {
    std::thread::spawn(move || {
        let result = job(&shared);
        {
            let mut core = lock(&shared);
            if let Some(id) = &busy_id {
                core.busy.remove(id);
            }
            match result {
                Ok(message) => core.set_status(message, false),
                Err(e) => core.set_status(e.to_string(), true),
            }
        }
        let _ = weak.upgrade_in_event_loop(move |ui| refresh(&ui, &shared));
    });
}

thread_local! {
    /// The action behind the open confirmation dialog. Lives on the UI thread only.
    static PENDING: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
}

fn ask(ui: &AppWindow, title: &str, text: &str, ok: &str, danger: bool, action: impl FnOnce() + 'static) {
    ui.set_confirm_title(title.into());
    ui.set_confirm_text(text.into());
    ui.set_confirm_ok(ok.into());
    ui.set_confirm_danger(danger);
    ui.set_confirm_open(true);
    PENDING.with(|p| *p.borrow_mut() = Some(Box::new(action)));
}

/// Downloads `remote` (or the latest version of `repo`) and installs it as entry `id`.
fn install_remote(
    shared: &Shared,
    id: &str,
    name: &str,
    repo: &str,
    spec: &SourceSpec,
    source: Source,
    remote: Option<RemoteVersion>,
) -> survivorsforge::Result<String> {
    let (client, downloads, addons) = {
        let core = lock(shared);
        (core.client(), core.downloads(), core.addons_dir().to_path_buf())
    };
    let remote = match remote {
        Some(remote) => remote,
        None => updates::resolve_remote(&client, repo, spec)?,
    };
    let zip = downloads.join(format!("{}.zip", unique_name()));
    client.download(&remote.download, &zip)?;
    let mut core = lock(shared);
    let request = Request { zip: &zip, id, name, source, version: remote.id.clone() };
    let result = install::install_zip(&addons, &mut core.state, request);
    let _ = std::fs::remove_file(&zip);
    result?;
    core.save_state();
    core.statuses.insert(id.to_string(), UpdateStatus::UpToDate);
    core.rescan();
    Ok(format!("{name} {} installed", remote.label))
}

/// The install job for an entry or verified addon, or `None` when it has no remote source.
/// (name, repo, spec, source, already known remote version)
type RemoteJob = (String, String, SourceSpec, Source, Option<RemoteVersion>);

fn remote_job(core: &Core, id: &str) -> Option<survivorsforge::Result<RemoteJob>> {
    let known = match core.statuses.get(id) {
        Some(UpdateStatus::UpdateAvailable(remote)) => Some(remote.clone()),
        _ => None,
    };
    if let Some(entry) = core.state.get(id) {
        return updates::remote_of(&entry.source, &core.registry)
            .map(|r| r.map(|(repo, spec)| (entry.name.clone(), repo, spec, entry.source.clone(), known)));
    }
    core.registry.get(id).map(|a| {
        Ok((a.name.clone(), a.repo.clone(), a.source.clone(), Source::Verified { registry_id: a.id.clone() }, None))
    })
}

fn on_row_action(ui: &AppWindow, shared: &Shared, id: String) {
    let job = {
        let mut core = lock(shared);
        if id.starts_with(UNMANAGED_PREFIX) || core.busy.contains(&id) {
            return;
        }
        match remote_job(&core, &id) {
            None => return,
            Some(Err(e)) => {
                core.set_status(e.to_string(), true);
                None
            }
            Some(Ok(job)) => {
                core.busy.insert(id.clone());
                core.set_status(format!("Installing {}...", job.0), false);
                Some(job)
            }
        }
    };
    refresh(ui, shared);
    let Some((name, repo, spec, source, remote)) = job else { return };
    run_bg(ui.as_weak(), shared.clone(), Some(id.clone()), move |shared| {
        install_remote(shared, &id, &name, &repo, &spec, source, remote)
    });
}

fn on_update_all(ui: &AppWindow, shared: &Shared) {
    let jobs: Vec<_> = {
        let mut core = lock(shared);
        let ids: Vec<String> =
            controller::update_all_ids(&core.statuses).into_iter().filter(|id| !core.busy.contains(id)).collect();
        let jobs: Vec<_> = ids
            .into_iter()
            .filter_map(|id| match remote_job(&core, &id) {
                Some(Ok(job)) => Some((id, job)),
                _ => None,
            })
            .collect();
        for (id, _) in &jobs {
            core.busy.insert(id.clone());
        }
        core.set_status(format!("Updating {} addons...", jobs.len()), false);
        jobs
    };
    refresh(ui, shared);
    let weak = ui.as_weak();
    run_bg(ui.as_weak(), shared.clone(), None, move |shared| {
        let total = jobs.len();
        let mut failed = Vec::new();
        for (id, (name, repo, spec, source, remote)) in jobs {
            if let Err(e) = install_remote(shared, &id, &name, &repo, &spec, source, remote) {
                failed.push(format!("{name}: {e}"));
            }
            lock(shared).busy.remove(&id);
            let s = shared.clone();
            let _ = weak.upgrade_in_event_loop(move |ui| refresh(&ui, &s));
        }
        if failed.is_empty() {
            Ok(format!("Updated {total} addons"))
        } else {
            Err(Error::Io(format!("{} of {total} updates failed. {}", failed.len(), failed.join(" | "))))
        }
    });
}

fn check_updates(ui: &AppWindow, shared: &Shared) {
    {
        let mut core = lock(shared);
        if core.checking || core.addons_dir().as_os_str().is_empty() {
            return;
        }
        core.checking = true;
        core.rescan();
        core.set_status("Checking for updates...".into(), false);
    }
    refresh(ui, shared);
    run_bg(ui.as_weak(), shared.clone(), None, |shared| {
        let (client, mut state, cache) = {
            let core = lock(shared);
            (core.client(), core.state.clone(), core.data_dir.join("cache"))
        };
        let (registry, origin) = registry::load(client.get_text(registry::REMOTE_URL), &cache);
        let results = updates::check_all(&client, &mut state, &registry);
        let self_update = match client.latest_release(APP_REPO) {
            Ok(Some(release)) => self_update::available(APP_VERSION, &release),
            _ => None,
        };

        let mut core = lock(shared);
        core.registry = registry;
        // Keep versions learned from adopted addons, but never undo an install that ran meanwhile.
        let mut changed = false;
        for checked in &state.entries {
            if let Some(real) = core.state.entries.iter_mut().find(|e| e.id == checked.id)
                && real.version != checked.version
                && real.version.starts_with(survivorsforge::state::TOC_PREFIX)
            {
                real.version = checked.version.clone();
                changed = true;
            }
        }
        if changed {
            core.save_state();
        }
        core.statuses = results.into_iter().collect();
        core.self_update = self_update;
        core.checking = false;
        core.rescan();

        let updates = controller::update_all_ids(&core.statuses).len();
        let failed = core.statuses.values().filter(|s| matches!(s, UpdateStatus::CheckFailed(_))).count();
        let mut message = match updates {
            0 => "All addons are up to date".to_string(),
            1 => "1 update available".to_string(),
            n => format!("{n} updates available"),
        };
        if failed > 0 {
            message += &format!(", {failed} checks failed");
        }
        match origin {
            Origin::Remote => {}
            Origin::Cache => message += " (verified list: cached copy)",
            Origin::Embedded => message += " (verified list: built-in copy)",
        }
        Ok(message)
    });
}

fn on_row_remove(ui: &AppWindow, shared: &Shared, id: String) {
    let (name, folders) = {
        let core = lock(shared);
        if let Some(folder) = id.strip_prefix(UNMANAGED_PREFIX) {
            let name = core.scan.iter().find(|s| s.folder == folder).map(|s| s.toc.title.clone());
            (name.unwrap_or_else(|| folder.to_string()), vec![folder.to_string()])
        } else if let Some(entry) = core.state.get(&id) {
            (entry.name.clone(), entry.folders.clone())
        } else {
            return;
        }
    };
    let text = format!(
        "These folders are moved to the recycle bin: {}.\n\nYour addon settings (SavedVariables in the WTF folder) stay untouched.",
        folders.join(", ")
    );
    let (weak, shared) = (ui.as_weak(), shared.clone());
    ask(ui, &format!("Remove {name}?"), &text, "Remove", true, move || {
        lock(&shared).busy.insert(id.clone());
        run_bg(weak, shared, Some(id.clone()), move |shared| {
            let mut core = lock(shared);
            let addons = core.addons_dir().to_path_buf();
            if id.starts_with(UNMANAGED_PREFIX) {
                install::remove_folders(&addons, &folders, &mut install::to_recycle_bin)?;
            } else {
                install::uninstall(&addons, &mut core.state, &id, &mut install::to_recycle_bin)?;
                core.statuses.remove(&id);
                core.save_state();
            }
            core.rescan();
            Ok(format!("{name} removed"))
        });
    });
}

fn on_install_zip(ui: &AppWindow, shared: &Shared) {
    // The button is hidden then; this only guards against a stray call.
    if !ui.get_file_dialog() {
        return;
    }
    let Some(file) = rfd::FileDialog::new().add_filter("Zip archive", &["zip"]).pick_file() else { return };
    let detected = match archive::analyze(&file) {
        Ok(detected) => detected,
        Err(e) => {
            let message = if e == Error::NoAddonFound { "No addon found in the zip".into() } else { e.to_string() };
            lock(shared).set_status(message, true);
            refresh(ui, shared);
            return;
        }
    };
    let plan = {
        let core = lock(shared);
        controller::plan_zip_install(&detected, &core.state, &core.scan)
    };
    let file_name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let title = if plan.is_update { "Update from zip" } else { "Install from zip" };
    let ok = if plan.is_update { "Update" } else { "Install" };
    let (weak, shared) = (ui.as_weak(), shared.clone());
    ask(ui, title, &plan.describe(), ok, false, move || {
        lock(&shared).busy.insert(plan.id.clone());
        run_bg(weak, shared, Some(plan.id.clone()), move |shared| {
            let mut core = lock(shared);
            let addons = core.addons_dir().to_path_buf();
            // Work on a copy: if the install fails, the replaced entries stay as they were.
            let mut state = core.state.clone();
            for (id, _) in &plan.takes_over {
                state.remove(id);
            }
            let request = Request {
                zip: &file,
                id: &plan.id,
                name: &plan.name,
                source: Source::Zip { file_name: file_name.clone() },
                version: format!("zip:{file_name}"),
            };
            install::install_zip(&addons, &mut state, request)?;
            core.state = state;
            for (id, _) in &plan.takes_over {
                core.statuses.remove(id);
            }
            core.statuses.insert(plan.id.clone(), UpdateStatus::Manual);
            core.save_state();
            core.rescan();
            Ok(format!("{} installed from {file_name}", plan.name))
        });
    });
}

fn on_repo_submit(ui: &AppWindow, shared: &Shared) {
    let repo = match normalize_repo(&ui.get_repo_input()) {
        Ok(repo) => repo,
        Err(e) => return ui.set_repo_error(e.to_string().into()),
    };
    ui.set_repo_error(SharedString::new());
    ui.set_repo_busy(true);
    let (weak, shared) = (ui.as_weak(), shared.clone());
    std::thread::spawn(move || {
        let (client, registry, downloads) = {
            let core = lock(&shared);
            (core.client(), core.registry.clone(), core.downloads())
        };
        let result = repo_detect::detect(&client, &registry, &repo, &mut |url| {
            let zip = downloads.join(format!("{}.zip", unique_name()));
            client.download(url, &zip)?;
            archive::analyze(&zip).map(|addons| (zip.clone(), addons)).inspect_err(|_| {
                let _ = std::fs::remove_file(&zip);
            })
        });
        let _ = weak.clone().upgrade_in_event_loop(move |ui| {
            ui.set_repo_busy(false);
            match result {
                Err(Error::NoAddonFound) => ui.set_repo_error("No addon found in this repo".into()),
                Err(e) => ui.set_repo_error(e.to_string().into()),
                Ok(detection) => {
                    ui.set_repo_open(false);
                    show_detection(&ui, &shared, detection);
                }
            }
        });
    });
}

fn show_detection(ui: &AppWindow, shared: &Shared, detection: repo_detect::Detection) {
    let source = match &detection.spec {
        SourceSpec::Release { asset } => format!("the latest release (asset {asset})"),
        SourceSpec::Tag => "the newest tag".into(),
        SourceSpec::Branch => "the default branch (every new commit counts as an update)".into(),
    };
    let names: Vec<&str> = detection.addons.iter().map(|a| a.name.as_str()).collect();
    let mut text = format!(
        "Addons found: {}.\nVersion: {}.\nUpdates come from {source}.",
        names.join(", "),
        detection.remote.label
    );
    let existing: Vec<&str> = {
        let core = lock(shared);
        names.iter().copied().filter(|n| core.scan.iter().any(|s| s.folder.eq_ignore_ascii_case(n))).collect()
    };
    if !existing.is_empty() {
        text += &format!("\n\nExisting folders get replaced: {}.", existing.join(", "));
    }
    let title = format!("Add {}?", detection.repo);
    let (weak, shared) = (ui.as_weak(), shared.clone());
    ask(ui, &title, &text, "Install", false, move || {
        let id = detection.entry_id();
        lock(&shared).busy.insert(id.clone());
        run_bg(weak, shared, Some(id.clone()), move |shared| {
            let mut core = lock(shared);
            let addons = core.addons_dir().to_path_buf();
            let name = detection.name();
            let request = Request {
                zip: &detection.zip,
                id: &id,
                name: &name,
                source: Source::GitHub { repo: detection.repo.clone(), spec: detection.spec.clone() },
                version: detection.remote.id.clone(),
            };
            let result = install::install_zip(&addons, &mut core.state, request);
            let _ = std::fs::remove_file(&detection.zip);
            result?;
            core.statuses.insert(id.clone(), UpdateStatus::UpToDate);
            core.save_state();
            core.rescan();
            Ok(format!("{name} added from {}", detection.repo))
        });
    });
}

/// The path the user typed, with `~` resolved. Shown back in the field so they see what is used.
fn typed_path(text: &str) -> PathBuf {
    let home = directories::UserDirs::new();
    config::expand_home(text, home.as_ref().map(|u| u.home_dir()))
}

fn on_settings_save(ui: &AppWindow, shared: &Shared) {
    let path = typed_path(&ui.get_settings_path());
    ui.set_settings_path(path.to_string_lossy().into_owned().into());
    let warning = match config::validate_addons_dir(&path) {
        Ok(warning) => warning,
        Err(e) => {
            ui.set_settings_message(e.to_string().into());
            ui.set_settings_message_error(true);
            return;
        }
    };
    let (saved, dir_changed) = {
        let mut core = lock(shared);
        let config = Config { addons_dir: path };
        let dir_changed = config.addons_dir != core.config.addons_dir;
        let saved = config.save(&core.data_dir);
        if dir_changed {
            core.switch_config(config);
        } else {
            core.config = config;
        }
        (saved, dir_changed)
    };
    match saved {
        Ok(()) => {
            let note = warning.map(|w| format!(" {w}")).unwrap_or_default();
            ui.set_settings_message(format!("Saved.{note}").into());
            ui.set_settings_message_error(false);
        }
        Err(e) => {
            ui.set_settings_message(format!("Could not save: {e}").into());
            ui.set_settings_message_error(true);
        }
    }
    refresh(ui, shared);
    if dir_changed {
        check_updates(ui, shared);
    }
}

fn on_setup_confirm(ui: &AppWindow, shared: &Shared) {
    let path = typed_path(&ui.get_setup_path());
    ui.set_setup_path(path.to_string_lossy().into_owned().into());
    match config::validate_addons_dir(&path) {
        Err(e) => {
            ui.set_setup_error(e.to_string().into());
            ui.set_setup_warning(SharedString::new());
            return;
        }
        // The first click shows the warning, the second ("use anyway") accepts it.
        Ok(Some(warning)) if ui.get_setup_warning().is_empty() => {
            ui.set_setup_error(SharedString::new());
            ui.set_setup_warning(warning.to_string().into());
            return;
        }
        Ok(_) => {}
    }
    let config = Config { addons_dir: path };
    {
        let mut core = lock(shared);
        if let Err(e) = config.save(&core.data_dir) {
            ui.set_setup_error(format!("Could not save the settings: {e}").into());
            return;
        }
        core.switch_config(config);
    }
    fill_settings(ui, shared);
    ui.set_needs_setup(false);
    refresh(ui, shared);
    check_updates(ui, shared);
}

fn fill_settings(ui: &AppWindow, shared: &Shared) {
    let core = lock(shared);
    ui.set_settings_path(core.config.addons_dir.to_string_lossy().into_owned().into());
}

fn on_self_update(ui: &AppWindow, shared: &Shared) {
    if !self_update::can_replace_self() {
        // In Game Mode a browser can't open over the app, so name the address instead.
        let opened = !ui.get_game_mode() && open::that(self_update::RELEASES_URL).is_ok();
        let message = if opened {
            "Opened the download page in your browser.".into()
        } else {
            format!("Download the new version from {}", self_update::RELEASES_URL)
        };
        lock(shared).set_status(message, false);
        return refresh(ui, shared);
    }
    let (client, update, work) = {
        let mut core = lock(shared);
        if !core.busy.is_empty() {
            core.set_status("Wait until the running installs finish, then update SurvivorsForge.".into(), true);
            drop(core);
            return refresh(ui, shared);
        }
        let Some(update) = core.self_update.clone() else { return };
        (core.client(), update, core.data_dir.join("update"))
    };
    ui.set_self_update_busy(true);
    let (weak, shared) = (ui.as_weak(), shared.clone());
    std::thread::spawn(move || match self_update::apply(&client, &update, &work) {
        Ok(()) => {
            let _ = weak.upgrade_in_event_loop(|_| self_update::restart());
        }
        Err(e) => {
            lock(&shared).set_status(format!("Self-update failed: {e}"), true);
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.set_self_update_busy(false);
                refresh(&ui, &shared);
            });
        }
    });
}

/// On Linux, rfd's dialogs go through xdg-desktop-portal over D-Bus. Without a session bus
/// (some minimal sessions) the dialog silently returns nothing, so the UI hides Browse instead.
fn file_dialog_available() -> bool {
    if !cfg!(target_os = "linux") {
        return true;
    }
    std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
        || std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|dir| Path::new(&dir).join("bus").exists())
}

fn pick_folder(start: &str) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if Path::new(start).is_dir() {
        dialog = dialog.set_directory(start);
    }
    dialog.pick_folder().map(|p| p.to_string_lossy().into_owned())
}

/// Wayland and X11 take the taskbar icon from the `.desktop` file whose name matches this id.
#[cfg(target_os = "linux")]
fn select_backend() -> Result<(), slint::PlatformError> {
    use slint::winit_030::winit::platform::{wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11};
    const APP_ID: &str = "survivorsforge";
    slint::BackendSelector::new()
        .with_winit_window_attributes_hook(|attrs| {
            let attrs = WindowAttributesExtWayland::with_name(attrs, APP_ID, APP_ID);
            WindowAttributesExtX11::with_name(attrs, APP_ID, APP_ID)
        })
        .select()
}

fn main() -> Result<(), slint::PlatformError> {
    #[cfg(target_os = "linux")]
    select_backend()?;
    let data_dir = config::data_dir();
    let config = Config::load(&data_dir);
    let needs_setup = config.is_none();
    let shared: Shared = Arc::new(Mutex::new(Core::new(data_dir, config)));

    let ui = AppWindow::new()?;
    ui.set_app_version(APP_VERSION.into());
    ui.set_self_update_in_place(self_update::can_replace_self());

    let game_mode = config::is_game_mode(
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
        std::env::var_os("GAMESCOPE_WAYLAND_DISPLAY").is_some(),
    );
    ui.set_game_mode(game_mode);
    ui.global::<Theme>().set_touch(game_mode);
    if game_mode {
        ui.window().set_fullscreen(true);
    }
    // The portal's file chooser may not show under gamescope, so Game Mode types paths.
    ui.set_file_dialog(file_dialog_available() && !game_mode);

    if needs_setup {
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let home = directories::UserDirs::new();
            let home = home.as_ref().map(|u| u.home_dir());
            // ponytail: first five only; a scroll list if people really keep more game copies
            let found: Vec<Suggestion> = config::find_addons_dirs(&config::search_roots(), 3)
                .into_iter()
                .take(5)
                .map(|p| Suggestion {
                    label: config::suggestion_label(&p, home).into(),
                    path: p.to_string_lossy().into_owned().into(),
                })
                .collect();
            let _ = weak.upgrade_in_event_loop(move |ui| {
                // A single find goes straight into the field, so Continue is the one thing to press.
                if let [only] = found.as_slice()
                    && ui.get_setup_path().is_empty()
                {
                    ui.set_setup_path(only.path.clone());
                }
                ui.set_setup_suggestions(ModelRc::new(VecModel::from(found)));
                ui.set_setup_searching(false);
            });
        });
    } else {
        ui.set_setup_searching(false);
    }
    ui.set_addons_example(if cfg!(windows) { r"...\benilla-config\AddOns" } else { ".../benilla-config/AddOns" }.into());
    ui.set_needs_setup(needs_setup);
    fill_settings(&ui, &shared);

    macro_rules! on {
        ($setter:ident, |$ui:ident, $shared:ident $(, $arg:ident)*| $body:expr) => {{
            let (weak, $shared) = (ui.as_weak(), shared.clone());
            ui.$setter(move |$($arg),*| {
                let $ui = weak.unwrap();
                let $shared = &$shared;
                $body
            });
        }};
    }

    on!(on_setup_browse, |ui, _shared| {
        if let Some(p) = pick_folder(&ui.get_setup_path()) {
            ui.set_setup_path(p.into());
            ui.set_setup_warning(SharedString::new());
        }
    });
    on!(on_setup_confirm, |ui, shared| on_setup_confirm(&ui, shared));
    on!(on_search_changed, |ui, shared, text| {
        lock(shared).search = text.to_string();
        refresh(&ui, shared);
    });
    on!(on_check_updates, |ui, shared| check_updates(&ui, shared));
    on!(on_update_all, |ui, shared| on_update_all(&ui, shared));
    on!(on_install_zip, |ui, shared| on_install_zip(&ui, shared));
    on!(on_add_repo, |ui, _shared| {
        ui.set_repo_input(SharedString::new());
        ui.set_repo_error(SharedString::new());
        ui.set_repo_open(true);
    });
    on!(on_repo_cancel, |ui, _shared| ui.set_repo_open(false));
    on!(on_repo_submit, |ui, shared| on_repo_submit(&ui, shared));
    on!(on_row_action, |ui, shared, id| on_row_action(&ui, shared, id.to_string()));
    on!(on_row_remove, |ui, shared, id| on_row_remove(&ui, shared, id.to_string()));
    on!(on_row_open, |_ui, shared, id| if let Some(repo) = lock(shared).repo_of(&id) {
        let _ = open::that(format!("https://github.com/{repo}"));
    });
    on!(on_confirm_accept, |ui, _shared| {
        ui.set_confirm_open(false);
        if let Some(action) = PENDING.with(|p| p.borrow_mut().take()) {
            action();
        }
    });
    on!(on_confirm_cancel, |ui, _shared| {
        ui.set_confirm_open(false);
        PENDING.with(|p| p.borrow_mut().take());
    });
    on!(on_settings_browse, |ui, _shared| {
        if let Some(p) = pick_folder(&ui.get_settings_path()) {
            ui.set_settings_path(p.into());
        }
    });
    on!(on_settings_save, |ui, shared| on_settings_save(&ui, shared));
    on!(on_self_update, |ui, shared| on_self_update(&ui, shared));

    // Own window frame (`no-frame`): moving, resizing and the caption buttons go through winit.
    on!(on_start_drag, |ui, _shared| {
        ui.window().with_winit_window(|w| {
            let _ = w.drag_window();
        });
    });
    on!(on_start_resize, |ui, _shared, direction| {
        use slint::winit_030::winit::window::ResizeDirection::*;
        let direction = [North, South, East, West, NorthEast, NorthWest, SouthEast, SouthWest][direction as usize];
        ui.window().with_winit_window(|w| {
            let _ = w.drag_resize_window(direction);
        });
    });
    on!(on_minimize, |ui, _shared| ui.window().set_minimized(true));
    on!(on_toggle_maximize, |ui, _shared| {
        ui.window().set_maximized(!ui.window().is_maximized());
    });
    // Quitting the loop instead of hiding: a hidden fullscreen window kept the app running.
    on!(on_close_window, |_ui, _shared| {
        let _ = slint::quit_event_loop();
    });

    refresh(&ui, &shared);
    if !needs_setup {
        check_updates(&ui, &shared);
    }
    ui.run()
}
