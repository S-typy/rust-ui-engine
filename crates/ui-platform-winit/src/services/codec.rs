use super::*;
pub(super) fn html_encode(fragment: &str) -> Vec<u8> {
    let mut header="Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n".to_owned();
    let start = header.len();
    let before = "<html><body><!--StartFragment-->";
    let after = "<!--EndFragment--></body></html>";
    let first = start + before.len();
    let last = first + fragment.len();
    let end = last + after.len();
    for (key, value) in [
        ("StartHTML:", start),
        ("EndHTML:", end),
        ("StartFragment:", first),
        ("EndFragment:", last),
    ] {
        let offset = header.find(key).unwrap() + key.len();
        header.replace_range(offset..offset + 10, &format!("{value:010}"));
    }
    format!("{header}{before}{fragment}{after}\0").into_bytes()
}
pub(super) fn html_decode(bytes: &[u8]) -> ServiceResult<String> {
    if bytes.len() > 4_194_304 + 1024 {
        return Err(invalid("Clipboard HTML exceeds its bound"));
    }
    let read = |header: &[u8], key: &[u8]| -> ServiceResult<usize> {
        let mut values = header
            .split(|b| matches!(*b, b'\n' | b'\r'))
            .filter_map(|line| line.strip_prefix(key))
            .map(|v| v.strip_suffix(b"\r").unwrap_or(v));
        let value = values
            .next()
            .ok_or_else(|| invalid("Missing clipboard HTML offset"))?;
        if values.next().is_some()
            || value.is_empty()
            || value.len() > 10
            || !value.iter().all(u8::is_ascii_digit)
        {
            return Err(invalid("Invalid clipboard HTML offset"));
        }
        std::str::from_utf8(value)
            .unwrap()
            .parse()
            .map_err(|_| invalid("Clipboard HTML offset overflow"))
    };
    // Find only the first offset before interpreting any body bytes. The full
    // header is bounded, so fragment content can never shadow header fields.
    let prefix = &bytes[..bytes.len().min(1024)];
    let start_line = prefix
        .split(|b| matches!(*b, b'\n' | b'\r'))
        .find(|line| line.starts_with(b"StartHTML:"))
        .ok_or_else(|| invalid("Missing clipboard HTML offset"))?;
    let start = read(start_line, b"StartHTML:")?;
    if start == 0 || start > 1024 || start > bytes.len() {
        return Err(invalid("Clipboard HTML header exceeds its bound"));
    }
    let header = &bytes[..start];
    if !header.is_ascii() {
        return Err(invalid("Clipboard HTML header is not ASCII"));
    }
    let confirmed = read(header, b"StartHTML:")?;
    if confirmed != start {
        return Err(invalid("Inconsistent clipboard HTML header"));
    }
    let end = read(header, b"EndHTML:")?;
    let first = read(header, b"StartFragment:")?;
    let last = read(header, b"EndFragment:")?;
    if !(start <= first && first <= last && last <= end && end <= bytes.len())
        || last - first > 4_194_304
    {
        return Err(invalid("Clipboard HTML offsets out of bounds"));
    }
    let fragment = std::str::from_utf8(&bytes[first..last])
        .map_err(|_| invalid("Clipboard HTML fragment is not UTF-8"))?;
    if fragment.contains('\0') {
        return Err(invalid("Clipboard HTML fragment contains NUL"));
    }
    Ok(fragment.into())
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn put(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
pub(super) fn dib_encode(image: &ClipboardImage) -> Vec<u8> {
    let mut result = vec![0; 124 + image.rgba.len()];
    put(&mut result, 0, 124);
    put(&mut result, 4, image.width);
    put(&mut result, 8, (-(image.height as i32)) as u32);
    result[12..14].copy_from_slice(&1u16.to_le_bytes());
    result[14..16].copy_from_slice(&32u16.to_le_bytes());
    put(&mut result, 16, 3);
    put(&mut result, 20, image.rgba.len() as u32);
    for (at, value) in [
        (40, 0x00ff0000),
        (44, 0x0000ff00),
        (48, 0x000000ff),
        (52, 0xff000000),
        (56, 0x73524742),
    ] {
        put(&mut result, at, value);
    }
    for (source, target) in image
        .rgba
        .chunks_exact(4)
        .zip(result[124..].chunks_exact_mut(4))
    {
        target.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
    }
    result
}
pub(super) fn dib_decode(bytes: &[u8]) -> ServiceResult<ClipboardImage> {
    if bytes.len() < 124
        || u32_at(bytes, 0) != 124
        || bytes[12..16] != [1, 0, 32, 0]
        || u32_at(bytes, 16) != 3
        || [
            (40, 0x00ff0000),
            (44, 0x0000ff00),
            (48, 0x000000ff),
            (52, 0xff000000),
        ]
        .into_iter()
        .any(|(at, v)| u32_at(bytes, at) != v)
        || u32_at(bytes, 32) != 0
        || u32_at(bytes, 56) != 0x73524742
        || u32_at(bytes, 112) != 0
        || u32_at(bytes, 116) != 0
    {
        return Err(ServiceError::new(
            ServiceErrorKind::Unsupported,
            "Clipboard image is not supported 32-bit DIBV5",
        ));
    }
    let width = u32_at(bytes, 4) as i32;
    let signed_height = u32_at(bytes, 8) as i32;
    let height = signed_height
        .checked_abs()
        .ok_or_else(|| invalid("Invalid DIB height"))?;
    if width <= 0 || width > 2048 || height == 0 || height > 2048 {
        return Err(invalid("DIB dimensions exceed limits"));
    }
    let length = width as usize * height as usize * 4;
    if (u32_at(bytes, 20) != 0 && u32_at(bytes, 20) as usize != length)
        || bytes.len() < 124 + length
    {
        return Err(invalid("Truncated DIB pixels"));
    }
    let mut rgba = vec![0; length];
    let row = width as usize * 4;
    for y in 0..height as usize {
        let source_y = if signed_height < 0 {
            y
        } else {
            height as usize - 1 - y
        };
        for (s, d) in bytes[124 + source_y * row..124 + (source_y + 1) * row]
            .chunks_exact(4)
            .zip(rgba[y * row..(y + 1) * row].chunks_exact_mut(4))
        {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
    }
    Ok(ClipboardImage {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}
