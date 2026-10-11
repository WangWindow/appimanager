//! Pure-Rust ELF parsing: locate the embedded SquashFS and read ELF sections.
//!
//! A type-2 AppImage is an ELF runtime with a SquashFS image appended right
//! after it. The payload therefore starts at the ELF's own size, computed the
//! same way as upstream `appimage_get_elf_size`:
//!
//! ```text
//! size = max(e_shoff + e_shentsize * e_shnum, last_section_offset + last_section_size)
//! ```
//!
//! The file is memory-mapped, so only the pages the ELF headers actually touch
//! are faulted in — opening a multi-hundred-megabyte AppImage never reads the
//! SquashFS body.

use std::path::Path;

use memmap2::Mmap;
use object::read::elf::{FileHeader, SectionHeader};
use object::{Endianness, FileKind, Object, ObjectSection};

use crate::error::{Error, Result};

/// Byte offset of the SquashFS payload: the size of the leading ELF.
pub(crate) fn offset(path: &Path) -> Result<u64> {
    let map = map(path)?;
    match FileKind::parse(&*map) {
        Ok(FileKind::Elf64) => elf_size::<object::elf::FileHeader64<Endianness>>(&map, path),
        Ok(FileKind::Elf32) => elf_size::<object::elf::FileHeader32<Endianness>>(&map, path),
        _ => Err(not_type2(path)),
    }
}

/// Bytes of a named ELF section, or `None` when it is absent or empty.
pub(crate) fn section(path: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    let map = map(path)?;
    let file = object::File::parse(&*map).map_err(|_| not_type2(path))?;
    let Some(section) = file.section_by_name(name) else {
        return Ok(None);
    };
    let bytes = section.data().map_err(|_| not_type2(path))?;
    Ok((!bytes.is_empty()).then(|| bytes.to_vec()))
}

fn elf_size<E: FileHeader<Endian = Endianness>>(data: &[u8], path: &Path) -> Result<u64> {
    let header = E::parse(data).map_err(|_| not_type2(path))?;
    let endian = header.endian().map_err(|_| not_type2(path))?;

    let sht_end = header.e_shoff(endian).into()
        + u64::from(header.e_shentsize(endian)) * u64::from(header.e_shnum(endian));

    let sections = header.sections(endian, data).map_err(|_| not_type2(path))?;
    let last_section_end = sections
        .iter()
        // SHT_NOBITS (for example .bss) contributes to the in-memory image
        // size, but occupies no bytes in the file. Counting it here can place
        // the reported AppImage payload offset past the actual SquashFS start.
        .map(|section| {
            if section.sh_type(endian) == object::elf::SHT_NOBITS {
                0
            } else {
                section.sh_offset(endian).into() + section.sh_size(endian).into()
            }
        })
        .max()
        .unwrap_or(0);

    let size = sht_end.max(last_section_end);
    if size == 0 {
        return Err(not_type2(path));
    }
    Ok(size)
}

fn map(path: &Path) -> Result<Mmap> {
    let file = std::fs::File::open(path)?;
    // Reject anything too small to hold an ELF identifier (e.g. `/dev/null`)
    // before mapping, since mapping a zero-length file fails with a vague error.
    if file.metadata()?.len() < 64 {
        return Err(not_type2(path));
    }
    // SAFETY: we only read the map, and treat concurrent external truncation as
    // a parse error rather than trusting the bytes.
    unsafe { Mmap::map(&file) }.map_err(Error::Io)
}

fn not_type2(path: &Path) -> Error {
    Error::InvalidImage {
        path: path.to_path_buf(),
        reason: "not a type-2 AppImage ELF",
    }
}

#[cfg(test)]
mod tests {
    use super::elf_size;
    use object::Endianness;
    use object::read::elf::{FileHeader, SectionHeader};
    use object::elf::FileHeader64;
    use std::path::Path;

    #[cfg(target_os = "linux")]
    #[test]
    fn nobits_sections_do_not_move_the_embedded_payload_offset() {
        let mut data = std::fs::read(std::env::current_exe().expect("test executable path"))
            .expect("read test executable");
        let expected = elf_size::<FileHeader64<Endianness>>(&data, Path::new("fixture.AppImage"))
            .expect("parse original ELF");
        let (section_table, section_size, bss_index) = {
            let header = FileHeader64::<Endianness>::parse(data.as_slice())
                .expect("parse ELF header");
            let endian = header.endian().expect("ELF byte order");
            let sections = header
                .sections(endian, data.as_slice())
                .expect("parse ELF sections");
            let bss_index = sections
                .iter()
                .position(|section| section.sh_type(endian) == object::elf::SHT_NOBITS)
                .expect("test binary contains a .bss section");
            (
                header.e_shoff(endian) as usize,
                usize::from(header.e_shentsize(endian)),
                bss_index,
            )
        };
        let bss_size_offset = section_table + bss_index * section_size + 32;
        data[bss_size_offset..bss_size_offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());

        let offset = elf_size::<FileHeader64<Endianness>>(&data, Path::new("fixture.AppImage"))
            .expect("parse ELF with an oversized NOBITS section");
        assert_eq!(offset, expected);
        assert!(offset <= data.len() as u64);
    }
}
