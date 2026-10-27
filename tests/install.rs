mod common;

use common::make_zip;
use std::path::{Path, PathBuf};
use survivorsforge::install::{install_zip, install_zip_with, uninstall, work_dir, Request};
use survivorsforge::state::{Source, State};
use survivorsforge::Error;

struct Env {
    _root: tempfile::TempDir,
    addons: PathBuf,
    zips: PathBuf,
}

fn env() -> Env {
    let root = tempfile::tempdir().unwrap();
    let addons = root.path().join("AddOns");
    let zips = root.path().join("zips");
    std::fs::create_dir_all(&addons).unwrap();
    std::fs::create_dir_all(&zips).unwrap();
    Env { addons, zips, _root: root }
}

fn request<'a>(zip: &'a Path, id: &'a str) -> Request<'a> {
    Request { zip, id, name: id, source: Source::Zip { file_name: "x.zip".into() }, version: "v1".into() }
}

fn staging_is_empty(addons: &Path) -> bool {
    let staging = work_dir(addons).join("staging");
    !staging.exists() || std::fs::read_dir(staging).unwrap().count() == 0
}

#[test]
fn fresh_install_creates_folders_and_state() {
    let e = env();
    let zip = make_zip(&e.zips, "ui.zip", &[("SurvivorUI/SurvivorUI.toc", ""), ("SurvivorPlates/SurvivorPlates.toc", "")]);
    let mut state = State::default();
    let entry = install_zip(&e.addons, &mut state, request(&zip, "ui")).unwrap();
    assert!(e.addons.join("SurvivorUI/SurvivorUI.toc").is_file());
    assert!(e.addons.join("SurvivorPlates/SurvivorPlates.toc").is_file());
    assert_eq!(entry.folders, ["SurvivorPlates", "SurvivorUI"]);
    assert_eq!(state.get("ui").unwrap().version, "v1");
    assert!(staging_is_empty(&e.addons));
}

#[test]
fn update_removes_stale_files() {
    let e = env();
    let mut state = State::default();
    let v1 = make_zip(&e.zips, "v1.zip", &[("A/A.toc", "1"), ("A/old.lua", "x")]);
    install_zip(&e.addons, &mut state, request(&v1, "a")).unwrap();
    let v2 = make_zip(&e.zips, "v2.zip", &[("A/A.toc", "2"), ("A/new.lua", "x")]);
    install_zip(&e.addons, &mut state, Request { version: "v2".into(), ..request(&v2, "a") }).unwrap();
    assert!(!e.addons.join("A/old.lua").exists());
    assert!(e.addons.join("A/new.lua").is_file());
    assert_eq!(state.entries.len(), 1);
    assert_eq!(state.get("a").unwrap().version, "v2");
    assert!(staging_is_empty(&e.addons));
}

#[test]
fn failed_move_rolls_back() {
    let e = env();
    let mut state = State::default();
    let v1 = make_zip(&e.zips, "v1.zip", &[("A/A.toc", "1"), ("A/old.lua", "x")]);
    install_zip(&e.addons, &mut state, request(&v1, "a")).unwrap();
    let v2 = make_zip(&e.zips, "v2.zip", &[("A/A.toc", "2"), ("B/B.toc", "")]);
    let mut calls = 0;
    let result = install_zip_with(&e.addons, &mut state, request(&v2, "a"), &mut |from, to| {
        calls += 1;
        if calls == 3 {
            return Err(std::io::Error::other("disk full"));
        }
        std::fs::rename(from, to)
    });
    assert!(result.is_err());
    assert!(e.addons.join("A/old.lua").is_file(), "old version restored");
    assert!(!e.addons.join("B").exists(), "half-moved new folder removed");
    assert_eq!(state.get("a").unwrap().version, "v1");
    assert!(staging_is_empty(&e.addons));
}

#[test]
fn refuses_folder_owned_by_other_addon() {
    let e = env();
    let mut state = State::default();
    let a = make_zip(&e.zips, "a.zip", &[("Shared/Shared.toc", "")]);
    install_zip(&e.addons, &mut state, request(&a, "first")).unwrap();
    let b = make_zip(&e.zips, "b.zip", &[("Shared/Shared.toc", "other")]);
    assert_eq!(
        install_zip(&e.addons, &mut state, request(&b, "second")),
        Err(Error::FolderOwnedBy("first".into()))
    );
    assert_eq!(std::fs::read_to_string(e.addons.join("Shared/Shared.toc")).unwrap(), "");
}

#[test]
fn uninstall_removes_folders_and_entry() {
    let e = env();
    let mut state = State::default();
    let zip = make_zip(&e.zips, "ui.zip", &[("X/X.toc", ""), ("Y/Y.toc", "")]);
    install_zip(&e.addons, &mut state, request(&zip, "ui")).unwrap();
    let mut deleted = Vec::new();
    uninstall(&e.addons, &mut state, "ui", &mut |p| {
        deleted.push(p.file_name().unwrap().to_string_lossy().into_owned());
        std::fs::remove_dir_all(p).map_err(Into::into)
    })
    .unwrap();
    assert_eq!(deleted, ["X", "Y"]);
    assert!(!e.addons.join("X").exists());
    assert!(state.get("ui").is_none());
}
