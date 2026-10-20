//! Turning a field into a typed value.

use chrono::NaiveDate;

use crate::options::{ColumnType, DateOrder, ImportOptions};
use crate::split::Field;

/// A cell's value after import.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// The field was empty.
    Empty,
    /// Text, as the field had it.
    Text(String),
    /// A number.
    Number(f64),
    /// A calendar date.
    Date(NaiveDate),
}

impl Value {
    /// The text of a [`Value::Text`], else `None`.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The number of a [`Value::Number`], else `None`.
    #[must_use]
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The date of a [`Value::Date`], else `None`.
    #[must_use]
    pub fn as_date(&self) -> Option<NaiveDate> {
        match self {
            Value::Date(d) => Some(*d),
            _ => None,
        }
    }
}

impl std::fmt::Display for Value {
    /// Text as is, numbers in Rust's shortest form, dates as `YYYY-MM-DD`,
    /// empty as nothing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Empty => Ok(()),
            Value::Text(text) => f.write_str(text),
            Value::Number(n) => write!(f, "{n}"),
            Value::Date(d) => write!(f, "{}", d.format("%Y-%m-%d")),
        }
    }
}

/// The value of `field` in a column of type `column_type`. A field the type
/// cannot read (a date column holding `n/a`) is kept as text rather than
/// lost. Never called for hidden columns; it returns text for them.
#[must_use]
pub fn convert(field: &Field, column_type: ColumnType, options: &ImportOptions) -> Value {
    let text = field.text.as_str();
    if text.is_empty() {
        return Value::Empty;
    }
    if field.quoted && options.quoted_as_text {
        return Value::Text(text.to_string());
    }
    let typed = match column_type {
        ColumnType::Standard => parse_number(text, options.decimal, options.thousands)
            .map(Value::Number)
            .or_else(|| {
                options
                    .detect_special_numbers
                    .then(|| standard_date(text, options.date_order))
                    .flatten()
                    .map(Value::Date)
            }),
        ColumnType::Date(order) => parse_date(text, order).map(Value::Date),
        ColumnType::Text | ColumnType::Hide => None,
    };
    typed.unwrap_or_else(|| Value::Text(text.to_string()))
}

/// `text` as a number written with the given decimal and thousands
/// separators: an optional sign (`+`, `-`, or the Unicode minus `−`), digits
/// with thousands separators only between digits, an optional decimal part
/// and an optional exponent (`1.5E3`). A trailing `-` and parentheses also
/// mean negative, as accounting exports write them. Spaces around are
/// ignored.
#[must_use]
pub fn parse_number(text: &str, decimal: char, thousands: Option<char>) -> Option<f64> {
    let mut s = text.trim();
    let mut negative = false;
    if let Some(inner) = s.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        negative = true;
        s = inner.trim();
    } else if let Some(rest) = s.strip_suffix('-') {
        negative = true;
        s = rest.trim_end();
    }
    if let Some(rest) = s.strip_prefix(['-', '−']) {
        if negative {
            return None;
        }
        negative = true;
        s = rest;
    } else if let Some(rest) = s.strip_prefix('+') {
        s = rest;
    }

    let is_thousands = |c: char| match thousands {
        Some(' ') => c == ' ' || c == '\u{A0}' || c == '\u{202F}',
        Some(t) => c == t,
        None => false,
    };
    let mut plain = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut digits = 0usize;
    let mut seen_decimal = false;
    let mut seen_exponent = false;
    let mut previous_digit = false;
    // Digits in the current group of the integer part, and whether a
    // thousands separator has been seen: groups after one have exactly three
    // digits, the first at most three.
    let mut group = 0usize;
    let mut grouped = false;
    let groups_ok = |group: usize, grouped: bool| !grouped || group == 3;
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            plain.push(c);
            digits += 1;
            if !seen_decimal && !seen_exponent {
                group += 1;
            }
            previous_digit = true;
        } else if c == decimal && !seen_decimal && !seen_exponent {
            if !groups_ok(group, grouped) {
                return None;
            }
            plain.push('.');
            seen_decimal = true;
            previous_digit = false;
        } else if is_thousands(c) && !seen_decimal && !seen_exponent {
            // Only between digits: "1.234" yes, ".234" or "1..234" no.
            if !previous_digit || !chars.peek().is_some_and(char::is_ascii_digit) {
                return None;
            }
            if (grouped && group != 3) || (!grouped && group > 3) {
                return None;
            }
            grouped = true;
            group = 0;
            previous_digit = false;
        } else if (c == 'e' || c == 'E') && digits > 0 && !seen_exponent {
            if !seen_decimal && !groups_ok(group, grouped) {
                return None;
            }
            plain.push('e');
            seen_exponent = true;
            previous_digit = false;
            if let Some(&sign) = chars.peek()
                && (sign == '+' || sign == '-')
            {
                plain.push(sign);
                chars.next();
            }
        } else {
            return None;
        }
    }
    if digits == 0
        || plain.ends_with(['e', '+', '-'])
        || (!seen_decimal && !seen_exponent && !groups_ok(group, grouped))
    {
        return None;
    }
    let value: f64 = plain.parse().ok()?;
    Some(if negative { -value } else { value })
}

/// `text` as a date in `order`: three numbers separated by `/`, `-`, `.` or
/// a space (`31/12/2026`, `2026-12-31`, `1.2.26`), or eight digits with no
/// separator (`31122026`, `20261231`). A two-digit year is 1930–2029, as in
/// LibreOffice. A time after the date (`2026-12-31 10:30`) is ignored.
#[must_use]
pub fn parse_date(text: &str, order: DateOrder) -> Option<NaiveDate> {
    let s = text.trim();
    // An ISO time after a `T` is not part of the date.
    let s = s.split('T').next().unwrap_or(s);
    let parts: Vec<&str> = s
        .split(['/', '-', '.', ' '])
        .filter(|p| !p.is_empty())
        .collect();
    let (a, b, c) = match parts.as_slice() {
        [compact] if compact.len() == 8 && compact.bytes().all(|b| b.is_ascii_digit()) => {
            match order {
                DateOrder::Ymd => (&compact[..4], &compact[4..6], &compact[6..]),
                DateOrder::Dmy | DateOrder::Mdy => (&compact[..2], &compact[2..4], &compact[4..]),
            }
        }
        [a, b, c, rest @ ..] if rest.iter().all(|p| looks_like_time(p)) => (*a, *b, *c),
        _ => return None,
    };
    // Day and month have one or two digits, the year two or four.
    let (day, month, year) = match order {
        DateOrder::Dmy => (a, b, c),
        DateOrder::Mdy => (b, a, c),
        DateOrder::Ymd => (c, b, a),
    };
    let digits = |p: &str, lengths: &[usize]| -> Option<u32> {
        (lengths.contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse().ok())
            .flatten()
    };
    let (day, month, two_digit, year) = (
        digits(day, &[1, 2])?,
        digits(month, &[1, 2])?,
        year.len() == 2,
        digits(year, &[2, 4])?,
    );
    let year = match (two_digit, year) {
        (true, y) if y < 30 => 2000 + y,
        (true, y) => 1900 + y,
        (false, y) => y,
    };
    NaiveDate::from_ymd_opt(i32::try_from(year).ok()?, month, day)
}

/// A date as a Standard column reads it: in `order`, or else in ISO 8601
/// (`2026-12-31`), which LibreOffice also accepts whatever the order. A
/// four-digit year first can be neither a day nor a month, so this never
/// changes how a date in `order` reads.
fn standard_date(text: &str, order: DateOrder) -> Option<NaiveDate> {
    let iso = |t: &str| {
        let t = t.trim().as_bytes();
        t.len() > 4 && t[..4].iter().all(u8::is_ascii_digit) && t[4] == b'-'
    };
    parse_date(text, order).or_else(|| {
        iso(text)
            .then(|| parse_date(text, DateOrder::Ymd))
            .flatten()
    })
}

/// A time of day after a date: `10:30`, `10:30:15`, `10:30:15.250`.
fn looks_like_time(part: &str) -> bool {
    part.contains(':')
        && part
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b':' || b == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn numbers_with_the_chosen_separators() {
        assert_eq!(parse_number("1234.56", '.', None), Some(1234.56));
        assert_eq!(parse_number("1,234.56", '.', Some(',')), Some(1234.56));
        assert_eq!(parse_number("1.234,56", ',', Some('.')), Some(1234.56));
        assert_eq!(parse_number("-45,20", ',', Some('.')), Some(-45.2));
        assert_eq!(
            parse_number("1 234 567,5", ',', Some(' ')),
            Some(1_234_567.5)
        );
        assert_eq!(parse_number("1\u{A0}234,5", ',', Some(' ')), Some(1234.5));
        assert_eq!(parse_number("1'234.5", '.', Some('\'')), Some(1234.5));
    }

    #[test]
    fn signs_accounting_negatives_and_exponents() {
        assert_eq!(parse_number("+7", '.', None), Some(7.0));
        assert_eq!(parse_number("−7", '.', None), Some(-7.0));
        assert_eq!(parse_number("7-", '.', None), Some(-7.0));
        assert_eq!(parse_number("(1,234.50)", '.', Some(',')), Some(-1234.5));
        assert_eq!(parse_number("1.5E3", '.', None), Some(1500.0));
        assert_eq!(parse_number("2e-2", '.', None), Some(0.02));
        assert_eq!(parse_number(".5", '.', None), Some(0.5));
    }

    #[test]
    fn what_is_not_a_number() {
        for text in [
            "",
            "-",
            "abc",
            "12abc",
            "1,2,3",
            "1,23",
            "1234,567",
            "12,3456.5",
            "1..2",
            ",5",
            "5,",
            "1.2.3",
            "--5",
            "(-5)",
            "1e",
            "€5",
            "5 %",
        ] {
            assert_eq!(parse_number(text, '.', Some(',')), None, "{text:?}");
        }
        // The wrong decimal separator is not silently read.
        assert_eq!(parse_number("45,20", '.', None), None);
    }

    #[test]
    fn dates_in_each_order() {
        assert_eq!(
            parse_date("31/12/2026", DateOrder::Dmy),
            Some(date(2026, 12, 31))
        );
        assert_eq!(
            parse_date("12/31/2026", DateOrder::Mdy),
            Some(date(2026, 12, 31))
        );
        assert_eq!(
            parse_date("2026-12-31", DateOrder::Ymd),
            Some(date(2026, 12, 31))
        );
        assert_eq!(
            parse_date("1.2.2026", DateOrder::Dmy),
            Some(date(2026, 2, 1))
        );
        assert_eq!(
            parse_date("31122026", DateOrder::Dmy),
            Some(date(2026, 12, 31))
        );
        assert_eq!(
            parse_date("20261231", DateOrder::Ymd),
            Some(date(2026, 12, 31))
        );
    }

    #[test]
    fn two_digit_years_and_times_after_the_date() {
        assert_eq!(
            parse_date("01/02/26", DateOrder::Dmy),
            Some(date(2026, 2, 1))
        );
        assert_eq!(
            parse_date("01/02/30", DateOrder::Dmy),
            Some(date(1930, 2, 1))
        );
        assert_eq!(
            parse_date("2026-12-31 10:30", DateOrder::Ymd),
            Some(date(2026, 12, 31))
        );
        assert_eq!(
            parse_date("2026-12-31T10:30:00", DateOrder::Ymd),
            Some(date(2026, 12, 31))
        );
    }

    #[test]
    fn what_is_not_a_date() {
        for (text, order) in [
            ("31/02/2026", DateOrder::Dmy),
            ("12/31/2026", DateOrder::Dmy),
            ("2026/12", DateOrder::Ymd),
            ("next week", DateOrder::Dmy),
            ("31/12/202", DateOrder::Dmy),
            ("2026-12-31", DateOrder::Dmy),
            ("1234", DateOrder::Dmy),
        ] {
            assert_eq!(parse_date(text, order), None, "{text:?} as {order:?}");
        }
    }

    #[test]
    fn standard_columns_read_numbers_then_dates_then_text() {
        let o = ImportOptions {
            decimal: ',',
            thousands: Some('.'),
            ..ImportOptions::default()
        };
        let v = |text: &str| convert(&Field::plain(text), ColumnType::Standard, &o);
        assert_eq!(v("1.234,56"), Value::Number(1234.56));
        assert_eq!(v("02/10/2026"), Value::Date(date(2026, 10, 2)));
        assert_eq!(
            v("2026-10-03"),
            Value::Date(date(2026, 10, 3)),
            "ISO in any order"
        );
        assert_eq!(
            v("2026/10/03"),
            Value::Text("2026/10/03".into()),
            "only ISO's dashes"
        );
        assert_eq!(v("NOMINA"), Value::Text("NOMINA".into()));
        assert_eq!(v(""), Value::Empty);
        let no_dates = ImportOptions {
            detect_special_numbers: false,
            ..o.clone()
        };
        assert_eq!(
            convert(&Field::plain("02/10/2026"), ColumnType::Standard, &no_dates),
            Value::Text("02/10/2026".into())
        );
    }

    #[test]
    fn text_date_and_quoted_columns() {
        let o = ImportOptions::default();
        assert_eq!(
            convert(&Field::plain("007"), ColumnType::Text, &o),
            Value::Text("007".into())
        );
        assert_eq!(
            convert(
                &Field::plain("12/31/2026"),
                ColumnType::Date(DateOrder::Mdy),
                &o
            ),
            Value::Date(date(2026, 12, 31))
        );
        assert_eq!(
            convert(&Field::plain("n/a"), ColumnType::Date(DateOrder::Dmy), &o),
            Value::Text("n/a".into())
        );
        let quoted = Field {
            text: "42".into(),
            quoted: true,
        };
        assert_eq!(
            convert(&quoted, ColumnType::Standard, &o),
            Value::Number(42.0)
        );
        let as_text = ImportOptions {
            quoted_as_text: true,
            ..o
        };
        assert_eq!(
            convert(&quoted, ColumnType::Standard, &as_text),
            Value::Text("42".into())
        );
    }

    #[test]
    fn display_writes_values_plainly() {
        assert_eq!(Value::Number(-45.2).to_string(), "-45.2");
        assert_eq!(Value::Date(date(2026, 1, 2)).to_string(), "2026-01-02");
        assert_eq!(Value::Empty.to_string(), "");
    }
}
