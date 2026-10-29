mod common;

use common::{clean_addon, zip};
use std::process::Command;

fn run(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_forge-scan")).args(args).output().unwrap();
    (out.status.code().unwrap(), String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn exit_codes_and_json() {
    let dir = tempfile::tempdir().unwrap();
    let clean = dir.path().join("clean.zip");
    std::fs::write(&clean, zip(&clean_addon())).unwrap();
    let (code, json) = run(&[clean.to_str().unwrap(), "--folders", "Lucky"]);
    assert_eq!(code, 0, "{json}");
    let report: forge_scan::Report = serde_json::from_str(&json).unwrap();
    assert!(!report.flagged());

    let (code, json) = run(&[clean.to_str().unwrap(), "--folders", "Other"]);
    assert_eq!(code, 1, "undeclared folder: {json}");
    assert!(json.contains("zip.undeclared_folder"));

    assert_eq!(run(&[dir.path().join("missing.zip").to_str().unwrap()]).0, 2);
    assert_eq!(run(&[]).0, 2);
}
