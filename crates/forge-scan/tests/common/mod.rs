#![allow(dead_code)]

use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

/// A zip in memory from `(entry name, content)` pairs. Names ending in `/` are folders.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (name, content) in entries {
        if name.ends_with('/') {
            writer.add_directory(*name, options).unwrap();
        } else {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

/// A clean addon `Lucky` with a toc, Lua, XML and a texture.
pub fn clean_addon() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("Lucky/", b""),
        ("Lucky/Lucky.toc", b"## Interface: 11200\n## Title: Lucky\n## SavedVariables: LuckyDB\nLucky.lua\nLucky.xml\n"),
        ("Lucky/Lucky.lua", b"local f = CreateFrame(\"Frame\")\nf:RegisterEvent(\"PLAYER_LOGIN\")\n"),
        ("Lucky/Lucky.xml", b"<Ui><Frame name=\"LuckyFrame\"/></Ui>"),
        ("Lucky/Textures/icon.tga", b"\x00\x00\x02\x00"),
    ]
}

pub fn folders(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}
