use super::*;

pub(super) fn decode(bytes: &[u8], options: DataFileOptions) -> Result<String, DataError> {
    if bytes.len() > options.max_file_bytes {
        return Err(DataError::Capacity(
            "serialized file byte budget exceeded".into(),
        ));
    }
    let invalid = || DataError::Adapter("invalid file encoding or conflicting BOM".into());
    match options.encoding {
        FileEncoding::Utf8 | FileEncoding::Utf8Bom => {
            if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
                return Err(invalid());
            }
            let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
            std::str::from_utf8(bytes)
                .map(str::to_owned)
                .map_err(|_| invalid())
        }
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            let little = options.encoding == FileEncoding::Utf16Le;
            let expected = if little { [0xff, 0xfe] } else { [0xfe, 0xff] };
            let opposite = if little { [0xfe, 0xff] } else { [0xff, 0xfe] };
            if bytes.starts_with(&opposite) || bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
                return Err(invalid());
            }
            let bytes = bytes.strip_prefix(&expected).unwrap_or(bytes);
            if !bytes.len().is_multiple_of(2) {
                return Err(invalid());
            }
            let units = bytes.chunks_exact(2).map(|pair| {
                if little {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            });
            let mut text = String::new();
            for ch in char::decode_utf16(units) {
                let ch = ch.map_err(|_| invalid())?;
                if text.len() + ch.len_utf8() > options.max_file_bytes {
                    return Err(DataError::Capacity(
                        "decoded UTF-8 byte budget exceeded".into(),
                    ));
                }
                text.push(ch);
            }
            Ok(text)
        }
    }
}
pub(super) fn encode(text: &str, options: DataFileOptions) -> Result<Vec<u8>, DataError> {
    let (prefix, bytes): (&[u8], usize) = match options.encoding {
        FileEncoding::Utf8 => (&[], text.len()),
        FileEncoding::Utf8Bom => (&[0xef, 0xbb, 0xbf], text.len()),
        FileEncoding::Utf16Le => (&[0xff, 0xfe], text.encode_utf16().count() * 2),
        FileEncoding::Utf16Be => (&[0xfe, 0xff], text.encode_utf16().count() * 2),
    };
    if bytes.saturating_add(prefix.len()) > options.max_file_bytes {
        return Err(DataError::Capacity(
            "encoded file byte budget exceeded".into(),
        ));
    }
    let mut output = Vec::with_capacity(bytes + prefix.len());
    output.extend_from_slice(prefix);
    match options.encoding {
        FileEncoding::Utf8 | FileEncoding::Utf8Bom => output.extend_from_slice(text.as_bytes()),
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            for unit in text.encode_utf16() {
                output.extend_from_slice(&if options.encoding == FileEncoding::Utf16Le {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
        }
    }
    Ok(output)
}
