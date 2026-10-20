//! The whole import without the dialog: bytes and options in, typed rows out.

use crate::charset::Charset;
use crate::options::{ColumnType, ImportOptions, Separators};
use crate::split::{Field, guess_separator, split};
use crate::value::{Value, convert};

/// One imported column: where it came from and how it was typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportedColumn {
    /// Its position in the text, from 0, counting hidden columns.
    pub source: usize,
    /// The type its values were read with.
    pub column_type: ColumnType,
}

/// What an import produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The columns kept (hidden ones are left out), in text order.
    pub columns: Vec<ImportedColumn>,
    /// One entry per row, one value per kept column. A short row is filled
    /// with [`Value::Empty`].
    pub rows: Vec<Vec<Value>>,
    /// The options the import used, to offer them again next time.
    pub options: ImportOptions,
}

/// Options for `bytes` with nothing chosen yet: `base` with the character
/// set detected and, when one is clear, the separator guessed.
#[must_use]
pub fn detect_options(bytes: &[u8], base: &ImportOptions) -> ImportOptions {
    let charset = Charset::detect(bytes);
    let sample = charset.decode(&bytes[..bytes.len().min(64 * 1024)]);
    let mut options = ImportOptions {
        charset,
        ..base.clone()
    };
    if let Some(separator) = guess_separator(&sample, options.quote) {
        options.separators = Separators {
            merge: false,
            ..Separators::single(separator)
        };
    }
    options
}

/// Imports all of `bytes` with `options`.
#[must_use]
pub fn import(bytes: &[u8], options: &ImportOptions) -> Imported {
    let text = options.charset.decode(bytes);
    let rows = split(&text, options, None);
    type_rows(&rows, options)
}

/// Types already split rows: drops hidden columns and converts each field by
/// its column's type.
#[must_use]
pub fn type_rows(rows: &[Vec<Field>], options: &ImportOptions) -> Imported {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let columns: Vec<ImportedColumn> = (0..width)
        .map(|source| ImportedColumn {
            source,
            column_type: options.column_type(source),
        })
        .filter(|c| c.column_type != ColumnType::Hide)
        .collect();
    let empty = Field::default();
    let rows = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|c| convert(row.get(c.source).unwrap_or(&empty), c.column_type, options))
                .collect()
        })
        .collect();
    Imported {
        columns,
        rows,
        options: options.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::DateOrder;
    use chrono::NaiveDate;

    const BANK: &[u8] = b"Fecha;Concepto;Importe;Saldo\n02/10/2026;COMPRA \"LA PE\xD1A\";-45,20;1.234,56\n03/10/2026;NOMINA;1.500,00;2.734,56\n";

    #[test]
    fn detect_finds_the_charset_and_separator() {
        let o = detect_options(BANK, &ImportOptions::default());
        assert_eq!(o.charset, Charset::Windows1252);
        assert!(o.separators.semicolon && !o.separators.comma);
    }

    #[test]
    fn detect_keeps_the_base_separators_when_none_is_clear() {
        let base = ImportOptions {
            separators: Separators::single('|'),
            ..ImportOptions::default()
        };
        assert_eq!(
            detect_options(b"one column\n", &base).separators,
            base.separators
        );
    }

    #[test]
    fn a_bank_export_imports_typed() {
        let mut o = detect_options(BANK, &ImportOptions::default());
        o.from_row = 2;
        o.decimal = ',';
        o.thousands = Some('.');
        o.set_column_type(3, ColumnType::Hide);
        let imported = import(BANK, &o);
        assert_eq!(
            imported
                .columns
                .iter()
                .map(|c| c.source)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            imported.rows,
            vec![
                vec![
                    Value::Date(NaiveDate::from_ymd_opt(2026, 10, 2).unwrap()),
                    Value::Text("COMPRA \"LA PEÑA\"".into()),
                    Value::Number(-45.2),
                ],
                vec![
                    Value::Date(NaiveDate::from_ymd_opt(2026, 10, 3).unwrap()),
                    Value::Text("NOMINA".into()),
                    Value::Number(1500.0),
                ],
            ]
        );
        assert_eq!(imported.options, o);
    }

    #[test]
    fn short_rows_are_filled_with_empty_values() {
        let o = ImportOptions {
            date_order: DateOrder::Ymd,
            ..ImportOptions::default()
        };
        let imported = import(b"a,b,c\nd\n", &o);
        assert_eq!(
            imported.rows[1],
            vec![Value::Text("d".into()), Value::Empty, Value::Empty]
        );
    }

    #[test]
    fn nothing_in_nothing_out() {
        let imported = import(b"", &ImportOptions::default());
        assert!(imported.columns.is_empty() && imported.rows.is_empty());
    }
}
