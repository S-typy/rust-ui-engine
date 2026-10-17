use super::*;

// Standard character widths and parity, GS1 General Specifications 26.0,
// sections 5.2.2 and 5.4.3.2. These are symbol definitions, not font outlines.
const EAN_A: [[u8; 4]; 10] = [
    [3, 2, 1, 1],
    [2, 2, 2, 1],
    [2, 1, 2, 2],
    [1, 4, 1, 1],
    [1, 1, 3, 2],
    [1, 2, 3, 1],
    [1, 1, 1, 4],
    [1, 3, 1, 2],
    [1, 2, 1, 3],
    [3, 1, 1, 2],
];
const EAN_PARITY: [&[u8; 6]; 10] = [
    b"AAAAAA", b"AABABB", b"AABBAB", b"AABBBA", b"ABAABB", b"ABBAAB", b"ABBBAA", b"ABABAB",
    b"ABABBA", b"ABBABA",
];
mod patterns;

pub(super) fn encode(kind: BarcodeSymbology, data: &str) -> Result<BarcodeSymbol, BarcodeError> {
    match kind {
        BarcodeSymbology::Ean13 => ean(data, 13),
        BarcodeSymbology::Ean8 => ean(data, 8),
        BarcodeSymbology::Code128B => code128(data),
    }
}
fn check_digit(digits: &[u8]) -> u8 {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    ((10 - sum % 10) % 10) as u8
}
fn ean(input: &str, length: usize) -> Result<BarcodeSymbol, BarcodeError> {
    if ![length - 1, length].contains(&input.len()) || !input.bytes().all(|b| b.is_ascii_digit()) {
        return Err(BarcodeError(
            "EAN requires exactly 7/8 or 12/13 ASCII digits for the selected format",
        ));
    }
    let expected = check_digit(&input.as_bytes()[..length - 1]);
    let mut data = input.to_owned();
    if input.len() == length - 1 {
        data.push(char::from(b'0' + expected));
    } else if input.as_bytes()[length - 1] - b'0' != expected {
        return Err(BarcodeError("EAN check digit does not match"));
    }
    let kind = if length == 13 {
        BarcodeSymbology::Ean13
    } else {
        BarcodeSymbology::Ean8
    };
    let mut out = BarcodeSymbol {
        symbology: kind,
        data,
        bars: Vec::new(),
        modules: 0,
        quiet_left: if length == 13 { 11 } else { 7 },
        quiet_right: 7,
    };
    let digits = out.data.as_bytes().to_vec();
    bits(&mut out, b"101", true);
    let count = if length == 13 { 6 } else { 4 };
    let first = usize::from(length == 13);
    for n in 0..count {
        let mut widths = EAN_A[usize::from(digits[first + n] - b'0')];
        if length == 13 && EAN_PARITY[usize::from(digits[0] - b'0')][n] == b'B' {
            widths.reverse();
        }
        widths_run(&mut out, &widths, false, false);
    }
    bits(&mut out, b"01010", true);
    for digit in &digits[first + count..] {
        widths_run(&mut out, &EAN_A[usize::from(digit - b'0')], true, false);
    }
    bits(&mut out, b"101", true);
    Ok(out)
}
fn bits(symbol: &mut BarcodeSymbol, pattern: &[u8], guard: bool) {
    let mut index = 0;
    while index < pattern.len() {
        let value = pattern[index];
        let start = index;
        while index < pattern.len() && pattern[index] == value {
            index += 1;
        }
        if value == b'1' {
            symbol.bars.push(BarcodeBar {
                start: symbol.modules + start as u16,
                width: (index - start) as u8,
                guard,
            });
        }
    }
    symbol.modules += pattern.len() as u16;
}
fn widths_run(symbol: &mut BarcodeSymbol, widths: &[u8], starts_dark: bool, guard: bool) {
    for (i, &width) in widths.iter().enumerate() {
        if (i % 2 == 0) == starts_dark {
            symbol.bars.push(BarcodeBar {
                start: symbol.modules,
                width,
                guard,
            });
        }
        symbol.modules += u16::from(width);
    }
}
fn code128(input: &str) -> Result<BarcodeSymbol, BarcodeError> {
    if input.is_empty() || input.len() > 128 || !input.bytes().all(|b| (32..=126).contains(&b)) {
        return Err(BarcodeError(
            "Code128B requires 1..128 printable ASCII characters",
        ));
    }
    let mut out = BarcodeSymbol {
        symbology: BarcodeSymbology::Code128B,
        data: input.into(),
        bars: Vec::new(),
        modules: 0,
        quiet_left: 10,
        quiet_right: 10,
    };
    widths_run(&mut out, &patterns::CODE128[104], true, false);
    let mut checksum = 104u32;
    for (i, b) in input.bytes().enumerate() {
        let code = usize::from(b - 32);
        checksum += (i as u32 + 1) * code as u32;
        widths_run(&mut out, &patterns::CODE128[code], true, false);
    }
    widths_run(
        &mut out,
        &patterns::CODE128[(checksum % 103) as usize],
        true,
        false,
    );
    widths_run(&mut out, &[2, 3, 3, 1, 1, 1, 2], true, false);
    Ok(out)
}
