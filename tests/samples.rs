//! Imports each file in `examples/data` with the options a user would pick
//! in the dialog, and checks the typed values that come back.

use chrono::NaiveDate;
use tv_text_import::{
    Charset, ColumnType, DateOrder, ImportOptions, Separators, Value, detect_options, import,
};

fn sample(name: &str) -> Vec<u8> {
    let path = format!("{}/examples/data/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn date(y: i32, m: u32, d: u32) -> Value {
    Value::Date(NaiveDate::from_ymd_opt(y, m, d).unwrap())
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

#[test]
fn bank_export_in_windows_1252_with_european_numbers() {
    let bytes = sample("bank_windows1252.csv");
    let detected = detect_options(&bytes, &ImportOptions::default());
    assert_eq!(detected.charset, Charset::Windows1252);
    assert_eq!(detected.separators, Separators::single(';'));

    let options = ImportOptions {
        from_row: 2,
        decimal: ',',
        thousands: Some('.'),
        ..detected
    };
    let imported = import(&bytes, &options);
    assert_eq!(imported.rows.len(), 3);
    assert_eq!(
        imported.rows[0],
        [
            date(2026, 10, 2),
            text("COMPRA \"LA PEÑA\" SUPERMERCADO"),
            Value::Number(-45.2),
            Value::Number(1234.56),
        ]
    );
    assert_eq!(imported.rows[1][2], Value::Number(1500.0));
}

#[test]
fn us_csv_with_quoted_commas_quotes_and_line_breaks() {
    let bytes = sample("us_sales.csv");
    let detected = detect_options(&bytes, &ImportOptions::default());
    assert_eq!(detected.separators, Separators::single(','));

    let options = ImportOptions {
        from_row: 2,
        thousands: Some(','),
        date_order: DateOrder::Mdy,
        ..detected
    };
    let imported = import(&bytes, &options);
    assert_eq!(imported.rows.len(), 5);
    assert_eq!(
        imported.rows[0],
        [
            date(2026, 10, 2),
            text("Smith, John"),
            text("Widget"),
            Value::Number(3.0),
            Value::Number(1234.5),
        ]
    );
    assert_eq!(imported.rows[1][2], text("Gadget \"Pro\""));
    assert_eq!(imported.rows[2][2], text("Cable\r\n(2 m)"));
    assert_eq!(imported.rows[2][4], Value::Number(-15.99));
    assert_eq!(
        imported.rows[4][1],
        text("  Padded  "),
        "quoted, so not trimmed"
    );
    assert_eq!(imported.rows[4][3], Value::Empty);
    assert_eq!(imported.rows[4][4], Value::Number(12000.0));
}

#[test]
fn tab_separated_utf8_with_bom_keeps_zip_codes_as_text() {
    let bytes = sample("cities.tsv");
    let mut options = detect_options(&bytes, &ImportOptions::default());
    assert_eq!(options.charset, Charset::Utf8);
    assert_eq!(options.separators, Separators::single('\t'));
    options.from_row = 2;
    options.set_column_type(0, ColumnType::Text);
    options.set_column_type(3, ColumnType::Date(DateOrder::Ymd));

    let imported = import(&bytes, &options);
    assert_eq!(imported.rows.len(), 5);
    assert_eq!(
        imported.rows[2],
        [
            text("01234"),
            text("São Paulo"),
            text("Brasil"),
            date(1554, 1, 25),
            Value::Number(11_451_245.0),
        ]
    );
}

#[test]
fn utf16_semicolons_and_a_hidden_column() {
    let bytes = sample("budget_utf16.csv");
    let mut options = detect_options(&bytes, &ImportOptions::default());
    assert_eq!(options.charset, Charset::Utf16Le);
    assert_eq!(options.separators, Separators::single(';'));
    options.from_row = 2;
    options.decimal = ',';
    options.thousands = Some('.');
    options.set_column_type(4, ColumnType::Hide);

    let imported = import(&bytes, &options);
    assert_eq!(imported.columns.len(), 4);
    assert_eq!(imported.rows[0][0], text("Alquiler"));
    assert_eq!(imported.rows[2][1], Value::Number(1200.0));
    assert_eq!(imported.rows[2][2], Value::Number(1150.5));
    assert_eq!(imported.rows[3][1], Value::Number(-35.0));
}

#[test]
fn space_aligned_columns_need_merged_delimiters() {
    let bytes = sample("aligned.txt");
    let options = ImportOptions {
        separators: Separators {
            space: true,
            merge: true,
            ..Separators::default()
        },
        from_row: 2,
        ..ImportOptions::default()
    };
    let imported = import(&bytes, &options);
    assert_eq!(imported.rows.len(), 4);
    assert_eq!(
        imported.rows[2],
        [
            Value::Number(3.0),
            text("gamma"),
            Value::Number(100.25),
            date(2026, 10, 3),
        ]
    );
}
