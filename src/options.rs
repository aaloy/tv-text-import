//! What the user chooses in the dialog: how the text is split into fields,
//! and how each column's fields become values.

use crate::charset::Charset;

/// Which characters end a field.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Separators {
    /// The tab character.
    pub tab: bool,
    /// `,`
    pub comma: bool,
    /// `;`
    pub semicolon: bool,
    /// A space.
    pub space: bool,
    /// Any further separators: every character of the text is one.
    pub other: String,
    /// Runs of separators count as one, so empty fields between them are
    /// not made.
    pub merge: bool,
}

impl Separators {
    /// Only `separator` (merging off). A tab, comma, semicolon or space sets
    /// its flag; any other character goes into `other`.
    #[must_use]
    pub fn single(separator: char) -> Self {
        let mut separators = Separators::default();
        match separator {
            '\t' => separators.tab = true,
            ',' => separators.comma = true,
            ';' => separators.semicolon = true,
            ' ' => separators.space = true,
            other => separators.other.push(other),
        }
        separators
    }

    /// Whether `c` ends a field.
    #[must_use]
    pub fn contains(&self, c: char) -> bool {
        let named = match c {
            '\t' => self.tab,
            ',' => self.comma,
            ';' => self.semicolon,
            ' ' => self.space,
            _ => false,
        };
        named || self.other.contains(c)
    }
}

/// The order of day, month and year in a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateOrder {
    /// `31/12/2026`
    Dmy,
    /// `12/31/2026`
    Mdy,
    /// `2026-12-31`
    Ymd,
}

impl DateOrder {
    /// Every order, in the order the dialog lists them.
    pub const ALL: [DateOrder; 3] = [DateOrder::Dmy, DateOrder::Mdy, DateOrder::Ymd];

    /// The name shown in the dialog.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            DateOrder::Dmy => "DMY",
            DateOrder::Mdy => "MDY",
            DateOrder::Ymd => "YMD",
        }
    }
}

/// How a column's fields become values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColumnType {
    /// A number when the field reads as one, else a date when
    /// [`ImportOptions::detect_special_numbers`] is on and it reads as one in
    /// [`ImportOptions::date_order`] or as an ISO 8601 date (`2026-12-31`),
    /// else text.
    #[default]
    Standard,
    /// Always text, even when it looks like a number (account numbers, codes
    /// with leading zeros).
    Text,
    /// A date in the given order; a field that is not one stays text.
    Date(DateOrder),
    /// Not imported.
    Hide,
}

impl ColumnType {
    /// Every type, in the order the dialog lists them.
    pub const ALL: [ColumnType; 6] = [
        ColumnType::Standard,
        ColumnType::Text,
        ColumnType::Date(DateOrder::Dmy),
        ColumnType::Date(DateOrder::Mdy),
        ColumnType::Date(DateOrder::Ymd),
        ColumnType::Hide,
    ];

    /// The name shown in the dialog and over the preview's columns.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ColumnType::Standard => "Standard",
            ColumnType::Text => "Text",
            ColumnType::Date(DateOrder::Dmy) => "Date (DMY)",
            ColumnType::Date(DateOrder::Mdy) => "Date (MDY)",
            ColumnType::Date(DateOrder::Ymd) => "Date (YMD)",
            ColumnType::Hide => "Hide",
        }
    }
}

/// Everything the import needs to turn bytes into typed rows.
///
/// [`Default`] is UTF-8, comma-separated, `"` quotes, spaces trimmed, `.` as
/// the decimal separator with no thousands separator, and day-month-year
/// dates. An application whose users expect other defaults (`;` and `1.234,56`
/// in much of Europe) sets them before showing the dialog, or passes the
/// options a previous import returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOptions {
    /// The character set the bytes are decoded in.
    pub charset: Charset,
    /// The first row imported, from 1. Earlier rows are skipped.
    pub from_row: usize,
    /// Which characters end a field.
    pub separators: Separators,
    /// The character that quotes a field (separators and line breaks inside
    /// are kept; the quote is doubled to include it), or `None` for no quoting.
    pub quote: Option<char>,
    /// Spaces around unquoted fields are removed.
    pub trim: bool,
    /// A quoted field is always text, whatever its column's type.
    pub quoted_as_text: bool,
    /// Standard columns recognise dates as well as numbers.
    pub detect_special_numbers: bool,
    /// The decimal separator of numbers.
    pub decimal: char,
    /// The thousands separator of numbers, if any. A space here also accepts
    /// a no-break space.
    pub thousands: Option<char>,
    /// The order Standard columns read dates in.
    pub date_order: DateOrder,
    /// Each column's type, by position; columns past the end are Standard.
    pub columns: Vec<ColumnType>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            charset: Charset::Utf8,
            from_row: 1,
            separators: Separators::single(','),
            quote: Some('"'),
            trim: true,
            quoted_as_text: false,
            detect_special_numbers: true,
            decimal: '.',
            thousands: None,
            date_order: DateOrder::Dmy,
            columns: Vec::new(),
        }
    }
}

impl ImportOptions {
    /// The type of column `index` (Standard past the end of `columns`).
    #[must_use]
    pub fn column_type(&self, index: usize) -> ColumnType {
        self.columns.get(index).copied().unwrap_or_default()
    }

    /// Sets the type of column `index`, growing `columns` with Standard as
    /// needed.
    pub fn set_column_type(&mut self, index: usize, column_type: ColumnType) {
        if self.columns.len() <= index {
            self.columns.resize(index + 1, ColumnType::Standard);
        }
        self.columns[index] = column_type;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_sets_the_named_flag_or_other() {
        assert!(Separators::single('\t').tab);
        assert!(Separators::single(';').semicolon);
        assert_eq!(Separators::single('|').other, "|");
        let s = Separators::single(';');
        assert!(s.contains(';') && !s.contains(','));
    }

    #[test]
    fn other_can_name_any_character_including_the_named_ones() {
        let s = Separators {
            other: "|,".into(),
            ..Separators::default()
        };
        assert!(s.contains('|') && s.contains(',') && !s.contains(';'));
    }

    #[test]
    fn column_types_default_to_standard_and_grow_on_set() {
        let mut o = ImportOptions::default();
        assert_eq!(o.column_type(3), ColumnType::Standard);
        o.set_column_type(2, ColumnType::Hide);
        assert_eq!(
            o.columns,
            vec![ColumnType::Standard, ColumnType::Standard, ColumnType::Hide]
        );
    }
}
