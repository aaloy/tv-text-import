//! Character sets a text file can be read in, and their decoding.
//!
//! The set covers what spreadsheets, banks and Windows tools export: UTF-8
//! (with or without a byte-order mark), UTF-16 with a byte-order mark,
//! Windows-1252 and Latin-1. Decoding never fails: a byte sequence that is
//! not valid in the chosen set becomes U+FFFD, so a wrong choice shows up in
//! the preview instead of refusing the file.

/// A character set the import can decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Charset {
    /// Unicode, UTF-8. A leading byte-order mark is skipped.
    Utf8,
    /// Unicode, UTF-16 little-endian. A leading byte-order mark is skipped.
    Utf16Le,
    /// Unicode, UTF-16 big-endian. A leading byte-order mark is skipped.
    Utf16Be,
    /// Western Europe, Windows-1252: Latin-1 plus `€`, `‘ ’`, `“ ”` and
    /// others in 0x80–0x9F.
    Windows1252,
    /// Western Europe, ISO-8859-1 (Latin-1).
    Latin1,
}

/// Windows-1252's characters for bytes 0x80–0x9F. The five unassigned
/// bytes (0x81, 0x8D, 0x8F, 0x90, 0x9D) map to the C1 control of the same
/// value, as the WHATWG encoding standard does.
const WINDOWS_1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
];

impl Charset {
    /// Every character set, in the order the dialog lists them.
    pub const ALL: [Charset; 5] = [
        Charset::Utf8,
        Charset::Utf16Le,
        Charset::Utf16Be,
        Charset::Windows1252,
        Charset::Latin1,
    ];

    /// The name shown in the dialog.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Charset::Utf8 => "Unicode (UTF-8)",
            Charset::Utf16Le => "Unicode (UTF-16 LE)",
            Charset::Utf16Be => "Unicode (UTF-16 BE)",
            Charset::Windows1252 => "Western (Windows-1252)",
            Charset::Latin1 => "Western (ISO-8859-1)",
        }
    }

    /// The likely character set of `bytes`: the one a byte-order mark names,
    /// else UTF-8 when the bytes are valid UTF-8, else Windows-1252 (which
    /// decodes any byte, and is what most non-Unicode exports are).
    #[must_use]
    pub fn detect(bytes: &[u8]) -> Charset {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            Charset::Utf8
        } else if bytes.starts_with(&[0xFF, 0xFE]) {
            Charset::Utf16Le
        } else if bytes.starts_with(&[0xFE, 0xFF]) {
            Charset::Utf16Be
        } else if std::str::from_utf8(bytes).is_ok() {
            Charset::Utf8
        } else {
            Charset::Windows1252
        }
    }

    /// Decodes `bytes` as text in this character set.
    #[must_use]
    pub fn decode(self, bytes: &[u8]) -> String {
        match self {
            Charset::Utf8 => {
                let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
                String::from_utf8_lossy(bytes).into_owned()
            }
            Charset::Utf16Le => decode_utf16(
                bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes),
                u16::from_le_bytes,
            ),
            Charset::Utf16Be => decode_utf16(
                bytes.strip_prefix(&[0xFE, 0xFF]).unwrap_or(bytes),
                u16::from_be_bytes,
            ),
            Charset::Windows1252 => bytes
                .iter()
                .map(|&b| match b {
                    0x80..=0x9F => WINDOWS_1252_HIGH[usize::from(b - 0x80)],
                    _ => char::from(b),
                })
                .collect(),
            Charset::Latin1 => bytes.iter().map(|&b| char::from(b)).collect(),
        }
    }
}

/// UTF-16 in the given byte order; an odd trailing byte becomes U+FFFD.
fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|&pair| unit(pair));
    let mut text: String = char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if bytes.len() % 2 == 1 {
        text.push(char::REPLACEMENT_CHARACTER);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_follows_a_byte_order_mark() {
        assert_eq!(Charset::detect(b"\xEF\xBB\xBFa;b"), Charset::Utf8);
        assert_eq!(Charset::detect(b"\xFF\xFEa\0"), Charset::Utf16Le);
        assert_eq!(Charset::detect(b"\xFE\xFF\0a"), Charset::Utf16Be);
    }

    #[test]
    fn detect_takes_valid_utf8_as_utf8_and_anything_else_as_windows_1252() {
        assert_eq!(Charset::detect("Peña;€".as_bytes()), Charset::Utf8);
        assert_eq!(Charset::detect(b"Pe\xF1a;\x80"), Charset::Windows1252);
        assert_eq!(Charset::detect(b""), Charset::Utf8);
    }

    #[test]
    fn utf8_skips_its_byte_order_mark_and_replaces_bad_bytes() {
        assert_eq!(Charset::Utf8.decode(b"\xEF\xBB\xBFabc"), "abc");
        assert_eq!(Charset::Utf8.decode(b"a\xFFb"), "a\u{FFFD}b");
    }

    #[test]
    fn utf16_in_both_byte_orders() {
        assert_eq!(Charset::Utf16Le.decode(b"\xFF\xFEP\0e\0\xF1\0a\0"), "Peña");
        assert_eq!(Charset::Utf16Be.decode(b"\xFE\xFF\0P\0e\0\xF1\0a"), "Peña");
        assert_eq!(Charset::Utf16Le.decode(b"\xAC\x20"), "€");
        assert_eq!(Charset::Utf16Le.decode(b"a\0b"), "a\u{FFFD}");
    }

    #[test]
    fn windows_1252_has_the_euro_and_curly_quotes_where_latin1_has_controls() {
        assert_eq!(
            Charset::Windows1252.decode(b"\x80 \x93x\x94 Pe\xF1a"),
            "€ “x” Peña"
        );
        assert_eq!(Charset::Latin1.decode(b"\x80Pe\xF1a"), "\u{80}Peña");
    }
}
