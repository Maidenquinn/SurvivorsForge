//! Addon packs: the addons of several zips in one zip, every addon folder at its root, the layout
//! of an AddOns folder. The platform builds packs for players of the browser version; the launcher
//! builds the same zip locally.

use crate::archive::{analyze_reader, safe_components};
use crate::{Error, Result};
use std::io::{Read, Seek, Write};
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

/// What went into a pack.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundled {
    /// Addon folder names, in input order.
    pub folders: Vec<String>,
    /// Sum of the compressed entries, about the size of the zip.
    pub packed_bytes: u64,
    pub unpacked_bytes: u64,
}

/// Copies the addon folders of every input zip into `out`. Entries are copied as they are (no
/// recompression). Fails when two inputs bring the same folder or the pack passes `max_bytes`.
pub fn bundle<R: Read + Seek, W: Write + Seek>(inputs: Vec<R>, out: W, max_bytes: u64) -> Result<Bundled> {
    let zip_err = |e: zip::result::ZipError| Error::Zip(e.to_string());
    let mut writer = ZipWriter::new(out);
    let mut done = Bundled { folders: Vec::new(), packed_bytes: 0, unpacked_bytes: 0 };
    for mut input in inputs {
        let addons = analyze_reader(&mut input)?;
        input.rewind()?;
        let mut zip = ZipArchive::new(input).map_err(zip_err)?;
        for addon in &addons {
            if done.folders.iter().any(|f| f.eq_ignore_ascii_case(&addon.name)) {
                return Err(Error::FolderTwice(addon.name.clone()));
            }
            writer.add_directory(format!("{}/", addon.name), SimpleFileOptions::default()).map_err(zip_err)?;
        }
        for i in 0..zip.len() {
            let file = zip.by_index_raw(i).map_err(zip_err)?;
            let path = safe_components(file.name())?.join("/");
            // The first addon that holds the entry gets it, like `archive::extract_addons`.
            let Some((addon, rel)) = addons.iter().find_map(|a| {
                let rel = if a.prefix.is_empty() { Some(path.as_str()) } else { path.strip_prefix(&a.prefix)?.strip_prefix('/') };
                rel.filter(|r| !r.is_empty()).map(|r| (a, r))
            }) else {
                continue;
            };
            let name = format!("{}/{rel}", addon.name);
            if file.is_dir() {
                writer.add_directory(format!("{name}/"), SimpleFileOptions::default()).map_err(zip_err)?;
                continue;
            }
            done.packed_bytes += file.compressed_size();
            done.unpacked_bytes += file.size();
            if done.packed_bytes > max_bytes {
                return Err(Error::TooLarge(max_bytes));
            }
            writer.raw_copy_file_rename(file, name).map_err(zip_err)?;
        }
        done.folders.extend(addons.into_iter().map(|a| a.name));
    }
    writer.finish().map_err(zip_err)?;
    Ok(done)
}
