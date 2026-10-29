mod common;

use common::make_zip;
use forge_core::Error;
use forge_core::archive::analyze_reader;
use forge_core::pack::bundle;
use std::fs::File;
use std::io::{Cursor, Read};

fn entries(zip: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
    let mut names: Vec<String> = (0..archive.len()).map(|i| archive.by_index(i).unwrap().name().to_string()).collect();
    names.sort();
    names
}

#[test]
fn every_addon_folder_lands_at_the_root() {
    let dir = tempfile::tempdir().unwrap();
    // A GitHub source zip (addon in a repo folder, extra files) and a release zip with two addons.
    let source = make_zip(dir.path(), "source.zip", &[
        ("BordoUI-2.8.6/README.md", "readme"),
        ("BordoUI-2.8.6/BordoUI.toc", "## Version: 2.8.6"),
        ("BordoUI-2.8.6/core/main.lua", "x"),
    ]);
    let release = make_zip(dir.path(), "release.zip", &[
        ("LICENSE", "mit"),
        ("SurvivorUI/SurvivorUI.toc", ""),
        ("SurvivorUI/Art/x.tga", "art"),
        ("SurvivorPlates/SurvivorPlates.toc", ""),
    ]);
    let mut out = Cursor::new(Vec::new());
    let done = bundle(vec![File::open(&source).unwrap(), File::open(&release).unwrap()], &mut out, 1_000_000).unwrap();
    assert_eq!(done.folders, ["BordoUI", "SurvivorPlates", "SurvivorUI"]);
    let zip = out.into_inner();
    assert_eq!(entries(&zip), [
        "BordoUI/", "BordoUI/BordoUI.toc", "BordoUI/README.md", "BordoUI/core/main.lua",
        "SurvivorPlates/", "SurvivorPlates/SurvivorPlates.toc",
        "SurvivorUI/", "SurvivorUI/Art/x.tga", "SurvivorUI/SurvivorUI.toc",
    ]);
    let found = analyze_reader(Cursor::new(&zip)).unwrap();
    assert!(found.iter().all(|a| a.prefix == a.name), "{found:?}");

    let mut archive = zip::ZipArchive::new(Cursor::new(&zip)).unwrap();
    let mut art = String::new();
    archive.by_name("SurvivorUI/Art/x.tga").unwrap().read_to_string(&mut art).unwrap();
    assert_eq!(art, "art", "copied without damage");
}

#[test]
fn the_same_folder_twice_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let a = make_zip(dir.path(), "a.zip", &[("Lib/Lib.toc", "")]);
    let b = make_zip(dir.path(), "b.zip", &[("x/lib/LIB.toc", "")]);
    let result = bundle(vec![File::open(&a).unwrap(), File::open(&b).unwrap()], Cursor::new(Vec::new()), 1_000_000);
    assert_eq!(result.unwrap_err(), Error::FolderTwice("lib".into()));
}

#[test]
fn packs_over_the_limit_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let big = "x".repeat(10_000);
    let a = make_zip(dir.path(), "a.zip", &[("Big/Big.toc", ""), ("Big/data.lua", big.as_str())]);
    let result = bundle(vec![File::open(&a).unwrap()], Cursor::new(Vec::new()), 10);
    assert_eq!(result.unwrap_err(), Error::TooLarge(10));
}

#[test]
fn a_zip_without_addon_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let a = make_zip(dir.path(), "a.zip", &[("readme.txt", "")]);
    assert_eq!(bundle(vec![File::open(&a).unwrap()], Cursor::new(Vec::new()), 1000).unwrap_err(), Error::NoAddonFound);
}
