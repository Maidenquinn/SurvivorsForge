use forge_scan::{Report, lua_rules};

/// Run in CI with `cargo test -p forge-scan --release -- --ignored`.
#[test]
#[ignore]
fn large_file_is_fast() {
    let chunk = "local frame = CreateFrame(\"Frame\")\nframe:SetScript(\"OnEvent\", function() print(arg1) end)\n";
    let source = chunk.repeat(5 * 1024 * 1024 / chunk.len());
    let started = std::time::Instant::now();
    let mut report = Report::default();
    lua_rules::check("big.lua", source.as_bytes(), 0, &mut report);
    assert!(started.elapsed().as_secs_f64() < 2.0, "took {:?}", started.elapsed());
}
