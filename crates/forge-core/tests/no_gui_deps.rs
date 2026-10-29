//! `forge-core` is used by the server, so it must never pull in the launcher's GUI stack.

#[test]
fn forge_core_has_no_gui_dependencies() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args(["tree", "-p", "forge-core", "-e", "normal", "--prefix", "none", "--offline"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let tree = String::from_utf8_lossy(&out.stdout);
    for forbidden in ["slint", "rfd", "trash", "self-replace"] {
        assert!(
            !tree.lines().any(|l| l.split_whitespace().next() == Some(forbidden)),
            "forge-core depends on {forbidden}"
        );
    }
}
