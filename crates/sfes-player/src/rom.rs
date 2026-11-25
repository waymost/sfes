//! Loading a ROM image from a file, or from inside a `.zip` archive.
//!
//! The cartridge loader drops a copier header itself (EMU-04), so this module only has to
//! produce the image's bytes.

use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
};

use anyhow::{Context, Result, bail};
use zip::ZipArchive;

/// What a zip archive starts with (a local file header).
const ZIP_MAGIC: [u8; 4] = *b"PK\x03\x04";
/// The largest ROM image accepted from an archive: the biggest cartridge plus its copier header,
/// with room to spare. It is checked against the bytes read, not the size the archive claims.
const ROM_LIMIT: usize = 16 * 1024 * 1024;
/// The extensions of SNES ROM images, lower case.
const EXTENSIONS: [&str; 4] = ["sfc", "smc", "swc", "fig"];

/// A ROM image and what to call it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoadedRom {
    /// The image, as the cartridge loader takes it.
    pub(crate) bytes: Vec<u8>,
    /// The file name, or `archive: entry` for a file from an archive.
    pub(crate) name: String,
}

/// Reads the ROM at `path`. A file that starts like a zip archive is unpacked, whatever its
/// extension; `entry` then names the archive member to use. Any other file is the image itself.
///
/// # Errors
///
/// Returns an error when the file cannot be read, when `entry` is given for a file that is not an
/// archive, or when the archive holds no single usable ROM (see [`from_zip`]).
pub(crate) fn load(path: &Path, entry: Option<&str>) -> Result<LoadedRom> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("rom")
        .to_owned();
    if !bytes.starts_with(&ZIP_MAGIC) {
        if entry.is_some() {
            bail!("--entry applies to zip archives, and {file_name} is not one");
        }
        return Ok(LoadedRom {
            bytes,
            name: file_name,
        });
    }
    let (member, image) = from_zip(&bytes, entry, ROM_LIMIT)
        .with_context(|| format!("unpacking {}", path.display()))?;
    Ok(LoadedRom {
        bytes: image,
        name: format!("{file_name}: {member}"),
    })
}

/// The ROM inside the zip archive `archive`: its name and bytes.
///
/// - With `entry`, that member (matched exactly).
/// - Otherwise the one member whose extension is `.sfc`, `.smc`, `.swc` or `.fig` (in any case);
///   failing that, the archive's only file, whatever it is called.
/// - No more than `limit` bytes are read from a member.
///
/// # Errors
///
/// Returns an error when the archive is damaged, when no member or several members qualify
/// (naming them), when the member is encrypted, or when it is longer than `limit`.
fn from_zip(archive: &[u8], entry: Option<&str>, limit: usize) -> Result<(String, Vec<u8>)> {
    let mut zip = ZipArchive::new(Cursor::new(archive)).context("reading the archive")?;
    let mut files = Vec::new();
    for index in 0..zip.len() {
        let member = zip.by_index_raw(index)?;
        if !member.is_dir() {
            let name = member
                .name()
                .with_context(|| format!("reading the name of member {index}"))?
                .into_owned();
            files.push((index, name, member.encrypted()));
        }
    }
    let names = files
        .iter()
        .map(|(_, name, _)| name.as_str())
        .collect::<Vec<_>>();
    let chosen = if let Some(wanted) = entry {
        files.iter().find(|(_, name, _)| name == wanted)
    } else {
        let roms = files
            .iter()
            .filter(|(_, name, _)| has_rom_extension(name))
            .collect::<Vec<_>>();
        match (roms.as_slice(), files.as_slice()) {
            ([only], _) => Some(*only),
            ([], [only]) => Some(only),
            ([], _) => None,
            (several, _) => bail!(
                "the archive holds several ROMs ({}); pick one with --entry",
                several
                    .iter()
                    .map(|(_, name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    };
    let Some(&(index, ref name, encrypted)) = chosen else {
        match entry {
            Some(wanted) => bail!(
                "the archive has no file `{wanted}`; it holds: {}",
                names.join(", ")
            ),
            None => bail!("the archive has no ROM; it holds: {}", names.join(", ")),
        }
    };
    if encrypted {
        bail!("`{name}` is encrypted, which is not supported");
    }
    let member = zip
        .by_index(index)
        .with_context(|| format!("opening `{name}`"))?;
    let mut image = Vec::new();
    let limit_plus_one = u64::try_from(limit).map_or(u64::MAX, |bytes| bytes.saturating_add(1));
    member
        .take(limit_plus_one)
        .read_to_end(&mut image)
        .with_context(|| format!("unpacking `{name}`"))?;
    if image.len() > limit {
        bail!("`{name}` is longer than {limit} bytes, too long for a ROM");
    }
    Ok((name.clone(), image))
}

/// Whether `name` ends in a ROM extension.
fn has_rom_extension(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

#[cfg(test)]
mod tests {
    use std::{io::Write, sync::Arc};

    use sfes_api::{Fast, Stop};
    use sfes_snes_core::Snes;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::*;

    /// An archive of `(name, bytes)` members, deflated, with directories named by a trailing `/`.
    fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in members {
            if name.ends_with('/') {
                writer.add_directory(*name, options).unwrap();
            } else {
                writer.start_file(*name, options).unwrap();
                writer.write_all(bytes).unwrap();
            }
        }
        writer.finish().unwrap().into_inner()
    }

    fn unpack(members: &[(&str, &[u8])], entry: Option<&str>) -> Result<(String, Vec<u8>)> {
        from_zip(&archive(members), entry, ROM_LIMIT)
    }

    #[test]
    fn the_only_rom_is_found_among_other_files() {
        let found = unpack(
            &[
                ("docs/", b""),
                ("readme.txt", b"hi"),
                ("Game.SFC", b"rom bytes"),
            ],
            None,
        )
        .unwrap();
        assert_eq!(found, ("Game.SFC".to_owned(), b"rom bytes".to_vec()));
    }

    #[test]
    fn every_rom_extension_counts() {
        for name in ["a.sfc", "a.smc", "a.swc", "a.fig", "a.SMC"] {
            assert_eq!(unpack(&[(name, b"x")], None).unwrap().0, name);
        }
    }

    #[test]
    fn a_lone_file_is_used_whatever_it_is_called() {
        let found = unpack(&[("game.bin", b"x")], None).unwrap();
        assert_eq!(found.0, "game.bin");
    }

    #[test]
    fn several_roms_are_an_error_that_names_them_until_one_is_chosen() {
        let members: [(&str, &[u8]); 2] = [("one.sfc", b"1"), ("two.smc", b"2")];
        let message = format!("{:#}", unpack(&members, None).unwrap_err());
        assert!(
            message.contains("one.sfc") && message.contains("two.smc"),
            "{message}"
        );
        assert_eq!(unpack(&members, Some("two.smc")).unwrap().1, b"2");
    }

    #[test]
    fn no_rom_among_several_files_is_an_error_that_lists_them() {
        let message = format!(
            "{:#}",
            unpack(&[("a.txt", b"a"), ("b.txt", b"b")], None).unwrap_err()
        );
        assert!(
            message.contains("a.txt") && message.contains("b.txt"),
            "{message}"
        );
    }

    #[test]
    fn a_missing_entry_is_an_error_that_lists_the_files() {
        let message = format!(
            "{:#}",
            unpack(&[("a.sfc", b"a")], Some("b.sfc")).unwrap_err()
        );
        assert!(
            message.contains("b.sfc") && message.contains("a.sfc"),
            "{message}"
        );
    }

    #[test]
    fn an_empty_archive_is_an_error() {
        assert!(unpack(&[], None).is_err());
    }

    #[test]
    fn a_member_over_the_limit_is_refused_whatever_the_archive_claims() {
        let bytes = archive(&[("big.sfc", &[0; 100])]);
        assert!(from_zip(&bytes, None, 99).is_err());
        assert_eq!(from_zip(&bytes, None, 100).unwrap().1.len(), 100);
    }

    #[test]
    fn an_encrypted_member_is_refused() {
        let mut bytes = archive(&[("a.sfc", b"a")]);
        // Set the encryption bit in the central directory's general purpose flags.
        let central = bytes
            .windows(4)
            .position(|window| window == b"PK\x01\x02")
            .unwrap();
        bytes[central + 8] |= 1;
        let message = format!("{:#}", from_zip(&bytes, None, ROM_LIMIT).unwrap_err());
        assert!(message.contains("encrypted"), "{message}");
    }

    #[test]
    fn damaged_data_is_an_error() {
        assert!(from_zip(b"PK\x03\x04 not really", None, ROM_LIMIT).is_err());
    }

    #[test]
    fn load_takes_a_plain_file_as_it_is_and_unpacks_an_archive_in_any_extension() {
        let directory =
            std::env::temp_dir().join(format!("sfes-player-rom-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let plain = directory.join("plain.sfc");
        fs::write(&plain, b"plain image").unwrap();
        let loaded = load(&plain, None).unwrap();
        assert_eq!(
            (loaded.bytes.as_slice(), loaded.name.as_str()),
            (&b"plain image"[..], "plain.sfc")
        );
        assert!(load(&plain, Some("x")).is_err());

        let zipped = directory.join("renamed.dat");
        fs::write(&zipped, archive(&[("inner.sfc", b"inner image")])).unwrap();
        let loaded = load(&zipped, None).unwrap();
        assert_eq!(loaded.bytes, b"inner image");
        assert_eq!(loaded.name, "renamed.dat: inner.sfc");
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_zipped_rom_with_a_copier_header_runs_like_the_plain_one() {
        let cart = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/assets/carts/bg-mode0.sfc"
        );
        let image = fs::read(cart).unwrap();
        let mut headered = vec![0; 512];
        headered.extend_from_slice(&image);
        let (_, unpacked) = unpack(&[("bg-mode0.smc", &headered)], None).unwrap();

        let size_after_frames = |image: Vec<u8>| {
            let mut machine = Snes::<Fast>::builder(Arc::from(image)).build().unwrap();
            machine.run(Stop::Frames(3));
            let frame = machine.video();
            (frame.width, frame.height)
        };
        assert_eq!(size_after_frames(unpacked), size_after_frames(image));
    }
}
