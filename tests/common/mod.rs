#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};

/// Builds a zip at `dir/name` from `(entry name, content)` pairs. Entry names ending in `/` are folders.
pub fn make_zip(dir: &Path, name: &str, entries: &[(&str, &str)]) -> PathBuf {
    let path = dir.join(name);
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (entry, content) in entries {
        if entry.ends_with('/') {
            zip.add_directory(*entry, options).unwrap();
        } else {
            zip.start_file(*entry, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
    }
    zip.finish().unwrap();
    path
}
