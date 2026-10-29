mod common;

use common::{clean_addon, folders, zip};
use forge_scan::{Report, Severity, scan};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

fn lucky(report_of: &[u8]) -> Report {
    scan(report_of, &folders(&["Lucky"]), &folders(&["LuckyOdds"]))
}

fn with(extra: &[(&'static str, &'static [u8])]) -> Vec<u8> {
    let mut entries = clean_addon();
    entries.extend_from_slice(extra);
    zip(&entries)
}

/// The single finding of `rule`, asserting its severity and file.
fn finding<'a>(report: &'a Report, rule: &str) -> &'a forge_scan::Finding {
    let hits: Vec<_> = report.findings.iter().filter(|f| f.rule == rule).collect();
    assert!(!hits.is_empty(), "no {rule} in {:#?}", report.findings);
    hits[0]
}

#[test]
fn clean_addon_has_no_warnings() {
    let report = lucky(&zip(&clean_addon()));
    assert!(!report.flagged(), "{:#?}", report.findings);
}

#[test]
fn path_traversal_is_critical() {
    for bad in ["../evil.lua", "/abs.lua", "C:/win.lua", "Lucky\\..\\x.lua"] {
        let report = lucky(&with(&[(bad, b"x")]));
        let f = finding(&report, "zip.path_traversal");
        assert_eq!((f.severity, f.file.as_str()), (Severity::Critical, bad));
    }
}

#[test]
fn symlink_is_critical() {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file("Lucky/Lucky.toc", SimpleFileOptions::default()).unwrap();
    writer.write_all(b"## Interface: 11200\n").unwrap();
    writer.add_symlink("Lucky/link", "/etc/passwd", SimpleFileOptions::default()).unwrap();
    let report = lucky(&writer.finish().unwrap().into_inner());
    assert_eq!(finding(&report, "zip.symlink").severity, Severity::Critical);
}

#[test]
fn renamed_exe_is_critical() {
    let report = lucky(&with(&[("Lucky/Textures/bg.tga", b"MZ\x90\x00\x03\x00\x00\x00")]));
    let f = finding(&report, "zip.executable");
    assert_eq!((f.severity, f.file.as_str()), (Severity::Critical, "Lucky/Textures/bg.tga"));
    for magic in [&b"\x7fELF\x02"[..], b"\xcf\xfa\xed\xfe", b"#!/bin/sh\n"] {
        let report = lucky(&with(&[("Lucky/x.lua", Box::leak(magic.to_vec().into_boxed_slice()))]));
        assert_eq!(finding(&report, "zip.executable").severity, Severity::Critical);
    }
}

#[test]
fn nested_zip_is_critical() {
    let inner: &'static [u8] = Box::leak(zip(&[("a.txt", b"x")]).into_boxed_slice());
    let report = lucky(&with(&[("Lucky/data.tga", inner)]));
    assert_eq!(finding(&report, "zip.nested_archive").severity, Severity::Critical);
    let report = lucky(&with(&[("Lucky/data.blp", b"\x1f\x8b\x08\x00")]));
    assert_eq!(finding(&report, "zip.nested_archive").severity, Severity::Critical);
}

#[test]
fn ratio_bomb_is_critical() {
    let zeros: &'static [u8] = Box::leak(vec![0u8; 2 * 1024 * 1024].into_boxed_slice());
    let report = lucky(&with(&[("Lucky/padding.txt", zeros)]));
    assert_eq!(finding(&report, "zip.bomb").severity, Severity::Critical);
    let small: &'static [u8] = Box::leak(vec![b' '; 100 * 1024].into_boxed_slice());
    assert!(lucky(&with(&[("Lucky/notes.txt", small)])).findings.iter().all(|f| f.rule != "zip.bomb"), "small files may compress well");
}

#[test]
fn bomb_detected_with_bounded_memory() {
    // 1 GB of zeros compresses to about 1 MB; reading stops at the per-file limit.
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file("Lucky/huge.txt", SimpleFileOptions::default().large_file(true)).unwrap();
    let chunk = vec![0u8; 1024 * 1024];
    for _ in 0..1024 {
        writer.write_all(&chunk).unwrap();
    }
    let bytes = writer.finish().unwrap().into_inner();
    let started = std::time::Instant::now();
    let report = lucky(&bytes);
    assert_eq!(finding(&report, "zip.bomb").severity, Severity::Critical);
    assert!(started.elapsed().as_secs() < 30, "stopped early");
}

#[test]
fn too_many_entries_is_critical() {
    let names: Vec<String> = (0..5001).map(|i| format!("Lucky/f{i}.txt")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
    let report = lucky(&zip(&entries));
    assert_eq!(finding(&report, "zip.bomb").severity, Severity::Critical);
}

#[test]
fn unknown_extension_warns() {
    let report = lucky(&with(&[("Lucky/tool.dll.bak", b"hello"), ("Lucky/run.ps1", b"Write-Host hi")]));
    let hits: Vec<_> = report.findings.iter().filter(|f| f.rule == "zip.extension").map(|f| f.file.as_str()).collect();
    assert_eq!(hits, ["Lucky/tool.dll.bak", "Lucky/run.ps1"]);
    assert!(report.findings.iter().filter(|f| f.rule == "zip.extension").all(|f| f.severity == Severity::Warn));
    assert!(!lucky(&with(&[("Lucky/LICENSE", b"MIT"), ("Lucky/README.md", b"# x")])).flagged(), "license and readme are fine");
}

#[test]
fn foreign_folder_is_critical() {
    let report = lucky(&with(&[("LuckyOdds/LuckyOdds.toc", b"## Interface: 11200\n")]));
    let f = finding(&report, "zip.foreign_folder");
    assert_eq!(f.severity, Severity::Critical);
    assert!(f.message.contains("LuckyOdds"));
}

#[test]
fn undeclared_folder_is_critical() {
    let report = lucky(&with(&[("Extra/Extra.toc", b"## Interface: 11200\n")]));
    assert_eq!(finding(&report, "zip.undeclared_folder").severity, Severity::Critical);
}

#[test]
fn missing_folder_warns() {
    let report = scan(&zip(&clean_addon()), &folders(&["Lucky", "LuckyPlates"]), &[]);
    let f = finding(&report, "zip.missing_folder");
    assert_eq!(f.severity, Severity::Warn);
    assert!(f.message.contains("LuckyPlates"));
}

#[test]
fn toc_missing_file_warns() {
    let report = lucky(&zip(&[
        ("Lucky/Lucky.toc", b"## Interface: 11200\nLucky.lua\nLibs\\Gone.lua\n# comment.lua\n"),
        ("Lucky/Lucky.lua", b"x = 1"),
    ]));
    let f = finding(&report, "zip.toc_missing_file");
    assert_eq!((f.severity, f.line, f.snippet.as_deref()), (Severity::Warn, Some(3), Some("Libs\\Gone.lua")));
    assert_eq!(report.findings.iter().filter(|f| f.rule == "zip.toc_missing_file").count(), 1);
}

#[test]
fn interface_mismatch_is_info() {
    let report = lucky(&zip(&[("Lucky/Lucky.toc", b"## Interface: 30300\n")]));
    assert_eq!(finding(&report, "zip.toc_interface").severity, Severity::Info);
    assert!(!report.flagged());
}

#[test]
fn macosx_is_info() {
    let report = lucky(&with(&[("__MACOSX/Lucky/._Lucky.lua", b"\x00\x05\x16\x07"), ("Lucky/.DS_Store", b"Bud1")]));
    assert_eq!(finding(&report, "zip.hidden").severity, Severity::Info);
    assert!(!report.flagged(), "{:#?}", report.findings);
}

#[test]
fn deep_nesting_warns() {
    let report = lucky(&with(&[("Lucky/a/b/c/d/e/f/g/h/i/j/k.lua", b"x")]));
    assert_eq!(finding(&report, "zip.deep_nesting").severity, Severity::Warn);
}

#[test]
fn encrypted_entry_is_critical() {
    let mut bytes = zip(&clean_addon());
    // Set the "encrypted" flag (bit 0) of every local and central header.
    for (sig, offset) in [(b"PK\x03\x04", 6), (b"PK\x01\x02", 8)] {
        let mut i = 0;
        while let Some(pos) = bytes[i..].windows(4).position(|w| w == sig) {
            bytes[i + pos + offset] |= 1;
            i += pos + 4;
        }
    }
    let report = lucky(&bytes);
    assert_eq!(finding(&report, "zip.unreadable").severity, Severity::Critical);
}

#[test]
fn garbage_is_unreadable() {
    let report = lucky(b"definitely not a zip");
    assert_eq!(finding(&report, "zip.unreadable").severity, Severity::Critical);
}
