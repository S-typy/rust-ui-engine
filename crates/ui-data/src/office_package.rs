//! Bounded preflight for ordinary ZIP packages used by document codecs.
//!
//! Decompression and CRC validation remain the ZIP reader's responsibility.
//! Preflight rejects duplicate directory entries before a reader can collapse them.

use crate::DataError;
use std::collections::BTreeSet;

fn bad() -> DataError {
    DataError::Adapter("invalid or unsupported Office ZIP directory".into())
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, DataError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, DataError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ))
}

/// Validate a single-disk ZIP32 central directory, without extracting any path.
/// ZIP64, self-extracting prefixes, encrypted parts and unknown compression are
/// outside this bounded profile. Only stored and deflated parts are accepted.
pub fn validate_zip_directory(
    bytes: &[u8],
    max_parts: usize,
    max_part_bytes: usize,
    max_expanded_bytes: usize,
) -> Result<(), DataError> {
    if bytes.len() < 22 {
        return Err(bad());
    }
    let end = (bytes.len().saturating_sub(65_557)..=bytes.len() - 22)
        .rev()
        .find(|offset| {
            bytes.get(*offset..*offset + 4) == Some(b"PK\x05\x06")
                && u16_at(bytes, *offset + 20)
                    .is_ok_and(|n| *offset + 22 + usize::from(n) == bytes.len())
        })
        .ok_or_else(bad)?;
    if u16_at(bytes, end + 4)? != 0 || u16_at(bytes, end + 6)? != 0 {
        return Err(bad());
    }
    let count = usize::from(u16_at(bytes, end + 10)?);
    if count > max_parts || count == 65535 || u16_at(bytes, end + 8)? as usize != count {
        return Err(bad());
    }
    let size = u32_at(bytes, end + 12)? as usize;
    let start = u32_at(bytes, end + 16)? as usize;
    if start.checked_add(size) != Some(end) {
        return Err(bad());
    }
    let mut cursor = start;
    let mut total = 0usize;
    let mut names = BTreeSet::new();
    let mut first_local = usize::MAX;
    for _ in 0..count {
        if bytes.get(cursor..cursor + 4) != Some(b"PK\x01\x02") {
            return Err(bad());
        }
        let flags = u16_at(bytes, cursor + 8)?;
        let method = u16_at(bytes, cursor + 10)?;
        if flags & 0x2041 != 0 || !matches!(method, 0 | 8) || u16_at(bytes, cursor + 34)? != 0 {
            return Err(bad());
        }
        let compressed = u32_at(bytes, cursor + 20)? as usize;
        let expanded = u32_at(bytes, cursor + 24)? as usize;
        if compressed == u32::MAX as usize
            || expanded == u32::MAX as usize
            || expanded > max_part_bytes
        {
            return Err(bad());
        }
        total = total.checked_add(expanded).ok_or_else(bad)?;
        if total > max_expanded_bytes {
            return Err(bad());
        }
        let name_len = usize::from(u16_at(bytes, cursor + 28)?);
        let extra_len = usize::from(u16_at(bytes, cursor + 30)?);
        let comment_len = usize::from(u16_at(bytes, cursor + 32)?);
        let name = bytes
            .get(cursor + 46..cursor + 46 + name_len)
            .ok_or_else(bad)?;
        let text = std::str::from_utf8(name).map_err(|_| bad())?;
        if text.is_empty()
            || text.starts_with('/')
            || text.contains(['\\', ':', '\0'])
            || text
                .trim_end_matches('/')
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
            || !names.insert(text.to_owned())
        {
            return Err(bad());
        }
        let local = u32_at(bytes, cursor + 42)? as usize;
        first_local = first_local.min(local);
        if local >= start
            || bytes.get(local..local + 4) != Some(b"PK\x03\x04")
            || u16_at(bytes, local + 6)? != flags
            || u16_at(bytes, local + 8)? != method
        {
            return Err(bad());
        }
        // With bit 3 clear the local header is authoritative too: accepting
        // contradictory CRC or lengths would give readers different views of
        // the same package. Streaming entries set bit 3 and may leave these
        // fields zero until their data descriptor; the directory bounds those
        // entries, and the ZIP reader validates decompression and CRC.
        if flags & 0x0008 == 0
            && (u32_at(bytes, local + 14)? != u32_at(bytes, cursor + 16)?
                || u32_at(bytes, local + 18)? as usize != compressed
                || u32_at(bytes, local + 22)? as usize != expanded)
        {
            return Err(bad());
        }
        let local_name = usize::from(u16_at(bytes, local + 26)?);
        let local_extra = usize::from(u16_at(bytes, local + 28)?);
        if bytes.get(local + 30..local + 30 + local_name) != Some(name)
            || local + 30 + local_name + local_extra + compressed > start
        {
            return Err(bad());
        }
        cursor = cursor
            .checked_add(46 + name_len + extra_len + comment_len)
            .ok_or_else(bad)?;
        if cursor > end {
            return Err(bad());
        }
    }
    if cursor != end || (count > 0 && first_local != 0) || (count == 0 && start != 0) {
        return Err(bad());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};
    fn package() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("one.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"sample").unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn valid_and_bounded() {
        let bytes = package();
        assert!(validate_zip_directory(&bytes, 5, 20, 100).is_ok());
        assert!(validate_zip_directory(&bytes, 0, 20, 100).is_err());
        assert!(validate_zip_directory(&bytes, 5, 1, 100).is_err());
        assert!(validate_zip_directory(&bytes, 5, 20, 1).is_err());
    }
    #[test]
    fn duplicate_central_entry_rejected_before_reader_dedup() {
        let mut bytes = package();
        let end = bytes.len() - 22;
        let start = u32_at(&bytes, end + 16).unwrap() as usize;
        let entry = bytes[start..end].to_vec();
        let mut footer = bytes[end..].to_vec();
        footer[8..10].copy_from_slice(&2u16.to_le_bytes());
        footer[10..12].copy_from_slice(&2u16.to_le_bytes());
        footer[12..16].copy_from_slice(&((entry.len() * 2) as u32).to_le_bytes());
        bytes.truncate(end);
        bytes.extend(entry);
        bytes.extend(footer);
        assert_eq!(zip::ZipArchive::new(Cursor::new(&bytes)).unwrap().len(), 1);
        assert!(validate_zip_directory(&bytes, 5, 20, 100).is_err());
    }
    #[test]
    fn local_name_mismatch_and_encryption_rejected() {
        let original = package();
        let mut bytes = original.clone();
        bytes[30] = b'x';
        assert!(validate_zip_directory(&bytes, 5, 20, 100).is_err());
        let mut bytes = original;
        let end = bytes.len() - 22;
        let start = u32_at(&bytes, end + 16).unwrap() as usize;
        bytes[start + 8] |= 1;
        assert!(validate_zip_directory(&bytes, 5, 20, 100).is_err());
    }
    #[test]
    fn local_crc_and_sizes_must_match_without_data_descriptor() {
        let original = package();
        assert_eq!(u16_at(&original, 6).unwrap() & 0x0008, 0);
        for offset in [14, 18, 22] {
            let mut bytes = original.clone();
            let changed = u32_at(&bytes, offset).unwrap() ^ 1;
            bytes[offset..offset + 4].copy_from_slice(&changed.to_le_bytes());
            assert!(
                validate_zip_directory(&bytes, 5, 20, 100).is_err(),
                "contradictory local header field at offset {offset}"
            );
        }
    }
    #[test]
    fn data_descriptor_allows_zero_local_crc_and_sizes() {
        let mut bytes = package();
        let end = bytes.len() - 22;
        let start = u32_at(&bytes, end + 16).unwrap() as usize;
        let mut descriptor = b"PK\x07\x08".to_vec();
        descriptor.extend_from_slice(&bytes[start + 16..start + 28]);
        let flags = u16_at(&bytes, 6).unwrap() | 0x0008;
        bytes[6..8].copy_from_slice(&flags.to_le_bytes());
        bytes[start + 8..start + 10].copy_from_slice(&flags.to_le_bytes());
        bytes[14..26].fill(0);
        let descriptor_len = descriptor.len();
        bytes.splice(start..start, descriptor);
        let end = bytes.len() - 22;
        bytes[end + 16..end + 20].copy_from_slice(&((start + descriptor_len) as u32).to_le_bytes());
        assert!(validate_zip_directory(&bytes, 5, 20, 100).is_ok());
        let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let mut decoded = Vec::new();
        archive
            .by_name("one.xml")
            .unwrap()
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, b"sample");
    }
    #[test]
    fn self_extracting_prefix_with_adjusted_offsets_is_rejected() {
        let mut bytes = package();
        let end = bytes.len() - 22;
        let start = u32_at(&bytes, end + 16).unwrap() as usize;
        let prefix = b"MZ-prefix";
        bytes[start + 42..start + 46].copy_from_slice(&(prefix.len() as u32).to_le_bytes());
        bytes[end + 16..end + 20].copy_from_slice(&((start + prefix.len()) as u32).to_le_bytes());
        let bytes = [prefix.as_slice(), bytes.as_slice()].concat();
        assert!(zip::ZipArchive::new(Cursor::new(&bytes)).is_ok());
        assert!(validate_zip_directory(&bytes, 5, 20, 100).is_err());
    }
}
