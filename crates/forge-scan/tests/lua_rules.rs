use forge_scan::{Report, Severity, lua_rules};

fn check(src: &str) -> Report {
    let mut report = Report::default();
    lua_rules::check("A/a.lua", src.as_bytes(), 0, &mut report);
    report
}

fn rules(src: &str) -> Vec<(String, Severity, Option<u32>)> {
    check(src).findings.into_iter().map(|f| (f.rule, f.severity, f.line)).collect()
}

fn has(src: &str, rule: &str, severity: Severity) {
    let found = rules(src);
    assert!(found.iter().any(|(r, s, _)| r == rule && *s == severity), "{rule}/{severity:?} not in {found:?} for {src}");
}

fn lacks(src: &str, rule: &str) {
    let found = rules(src);
    assert!(found.iter().all(|(r, _, _)| r != rule), "unexpected {rule} in {found:?} for {src}");
}

#[test]
fn forbidden_global_positive() {
    for src in ["os.execute('x')", "local f = io.open('x')", "debug.getinfo(1)", "require('socket')", "x = getfenv(0)", "loadlib('a', 'b')",
        "dofile('x')", "local s = string.dump(f)", "collectgarbage()"] {
        has(src, "lua.forbidden_global", Severity::Critical);
    }
}

#[test]
fn forbidden_global_negative() {
    for src in ["x.os = 1", "self:io()", "local t = { os = 1 }", "local myos = 1", "iostat()", "-- os.execute('x')", "local s = 'os.exit()'"] {
        lacks(src, "lua.forbidden_global");
    }
}

#[test]
fn line_numbers_are_reported() {
    let found = rules("local a = 1\n\nos.exit()\n");
    assert!(found.contains(&("lua.forbidden_global".into(), Severity::Critical, Some(3))), "{found:?}");
}

#[test]
fn forbidden_name_string_positive() {
    has("local n = 'os'", "lua.forbidden_name_string", Severity::Critical);
    has(r#"local n = "\108\111\097\100\115\116\114\105\110\103""#, "lua.forbidden_name_string", Severity::Critical);
    has("local n = 'Run' .. 'Script'", "lua.forbidden_name_string", Severity::Critical);
}

#[test]
fn forbidden_name_string_negative() {
    lacks("local n = 'osmosis'", "lua.forbidden_name_string");
    lacks("local n = 'o' .. x .. 's'", "lua.forbidden_name_string");
}

#[test]
fn dynamic_code_positive() {
    has("loadstring('return 1')()", "lua.dynamic_code", Severity::Warn);
    has("loadstring(code)()", "lua.dynamic_code", Severity::Critical);
    has("RunScript(a .. b)", "lua.dynamic_code", Severity::Critical);
    has("local l = loadstring", "lua.dynamic_code", Severity::Critical);
}

#[test]
fn dynamic_code_negative() {
    lacks("frame.loadstring = nil", "lua.dynamic_code");
    lacks("-- loadstring(x)", "lua.dynamic_code");
}

#[test]
fn folded_concat_and_dynamic_global() {
    let src = r#"local o = "o".."s"; _G[o].execute("x")"#;
    has(src, "lua.forbidden_name_string", Severity::Critical);
    has(src, "lua.dynamic_global", Severity::Warn);
}

#[test]
fn dynamic_global_positive() {
    has("local f = getglobal(name)", "lua.dynamic_global", Severity::Warn);
    has("setglobal(n, 1)", "lua.dynamic_global", Severity::Warn);
    has("local v = _G[k]", "lua.dynamic_global", Severity::Warn);
}

#[test]
fn dynamic_global_negative() {
    lacks("local f = getglobal('LuckyFrame')", "lua.dynamic_global");
    lacks("local v = _G['LuckyDB']", "lua.dynamic_global");
    lacks("local v = t[k]", "lua.dynamic_global");
}

#[test]
fn global_metatable_positive() {
    has("setmetatable(_G, { __index = f })", "lua.global_metatable", Severity::Critical);
    has("rawset(_G, k, v)", "lua.global_metatable", Severity::Critical);
}

#[test]
fn global_metatable_negative() {
    lacks("setmetatable(t, mt)", "lua.global_metatable");
    lacks("rawset(_G, 'LuckyDB', {})", "lua.global_metatable");
}

#[test]
fn chat_send_positive() {
    has("SendChatMessage('hi', 'GUILD')", "lua.chat_send", Severity::Warn);
    has("GuildInvite(name)", "lua.chat_send", Severity::Warn);
}

#[test]
fn chat_send_negative() {
    lacks("DEFAULT_CHAT_FRAME:AddMessage('hi')", "lua.chat_send");
    lacks("frame.SendChatMessage = nil", "lua.chat_send");
}

#[test]
fn api_override_positive() {
    has("SendChatMessage = function() end", "lua.api_override", Severity::Warn);
    has("function ChatFrame_OnEvent(event) end", "lua.api_override", Severity::Warn);
}

#[test]
fn api_override_negative() {
    lacks("local SendChatMessage = SendChatMessage", "lua.api_override");
    lacks("if CreateFrame == nil then end", "lua.api_override");
}

#[test]
fn url_positive() {
    has("local u = 'https://evil.test/x'", "lua.url", Severity::Warn);
    has("print('join discord.gg/abc')", "lua.url", Severity::Warn);
}

#[test]
fn url_negative() {
    lacks("-- see https://example.test", "lua.url");
    lacks("local s = 'httpd'", "lua.url");
}

#[test]
fn obfuscation_char_positive() {
    has("local s = string.char(104,101,108,108,111,32,119,111)", "lua.obfuscation_char", Severity::Warn);
}

#[test]
fn obfuscation_char_negative() {
    lacks("local s = string.char(65, 66)", "lua.obfuscation_char");
}

#[test]
fn obfuscation_escape_positive() {
    has(r#"local s = "\104\101\108\108\111\032\119\111""#, "lua.obfuscation_escape", Severity::Warn);
}

#[test]
fn obfuscation_escape_negative() {
    lacks(r#"local s = "line\nnext\ttab""#, "lua.obfuscation_escape");
}

#[test]
fn obfuscation_entropy_positive() {
    // Base64-looking noise from a small LCG.
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut seed: u32 = 12345;
    let blob: String = (0..300)
        .map(|_| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ALPHABET[(seed >> 16) as usize % ALPHABET.len()] as char
        })
        .collect();
    has(&format!("local s = [[{blob}]]"), "lua.obfuscation_entropy", Severity::Warn);
}

#[test]
fn obfuscation_entropy_negative() {
    let prose = "This addon shows your luck for rare cards and keeps a small history of the last runs. ".repeat(4);
    lacks(&format!("local s = '{prose}'"), "lua.obfuscation_entropy");
}

#[test]
fn long_line_positive() {
    has(&format!("local x = {}", "1 + ".repeat(600)), "lua.long_line", Severity::Warn);
}

#[test]
fn long_line_negative() {
    lacks("local x = 1\n", "lua.long_line");
}

#[test]
fn parse_error_positive() {
    has("local function (", "lua.parse_error", Severity::Warn);
}

#[test]
fn parse_error_negative() {
    lacks("local function f() return 1 end", "lua.parse_error");
}

#[test]
fn binary_positive() {
    let mut report = Report::default();
    lua_rules::check("a.lua", b"\x1bLuaQ\x00\x01", 0, &mut report);
    assert_eq!(report.findings[0].rule, "lua.binary");
    assert_eq!(report.findings[0].severity, Severity::Critical);
}

#[test]
fn binary_negative() {
    lacks("print('no nul here')", "lua.binary");
}

#[test]
fn comments_and_substrings_ignored() {
    for src in ["-- os.execute('rm')", "--[[ io.open('x') loadstring(y) ]]", "local myos, iostat = 1, 2", "local t = 'a'"] {
        let found = rules(src);
        assert!(found.is_empty(), "{src}: {found:?}");
    }
}

#[test]
fn clean_addon_has_no_warnings() {
    let report = check(include_str!("fixtures/lua/clean_addon.lua"));
    assert!(!report.flagged(), "{:#?}", report.findings);
}
