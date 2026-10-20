# tv-text-import

A Text Import dialog for [turbo-vision](https://github.com/aovestdipaperino/turbo-vision-4-rust),
after LibreOffice Calc's. Give it the bytes of a delimited text file (CSV,
TSV, a bank's semicolon export…). The user sets how to read it while a preview
follows every change. On OK you get typed rows back.

```
╔═[■]═══════════════ Text Import - bank_windows1252.csv ═══════════════════╗
║ ┌─ Import ─────────────────────────────────────────────────────────────┐ ║
║ │ Character set  Western (Windows-1252)  ▼  From row       2▲▼         │ ║
║ │ Decimal        Comma ,  ▼  Thousands  Point .       ▼  Dates  DMY  ▼ │ ║
║ │               [ ] Quoted fields as text  [X] Detect special numbers  │ ║
║ └──────────────────────────────────────────────────────────────────────┘ ║
║ ┌─ Separator options ──────────────────────────────────────────────────┐ ║
║ │ [ ] Tab  [ ] Comma  [X] Semicolon  [ ] Space  Other                  │ ║
║ │ [ ] Merge delimiters  [X] Trim spaces  String delimiter  "     ▼     │ ║
║ └──────────────────────────────────────────────────────────────────────┘ ║
║ ┌─ Fields ─────────────────────────────────────────────────────────────┐ ║
║ │ Column type  Standard    ▼                                           │ ║
║ │  ║Standard  │Standard                │Standard│Standard              │ ║
║ │ 2║02/10/2026│COMPRA "LA PEÑA" SUPERME│-45,20  │1.234,56              │ ║
║ │ 3║03/10/2026│NOMINA OCTUBRE          │1.500,00│2.734,56              │ ║
║ │ 4║05/10/2026│RECIBO LUZ              │-62,31  │2.672,25              │ ║
║ └──────────────────────────────────────────────────────────────────────┘ ║
║                             OK    ▀   Cancel  ▀                          ║
╚══════════════════════════════════════════════════════════════════════════╝
```

## What it does

- **Character set**: UTF-8, UTF-16 LE/BE, Windows-1252 and ISO-8859-1. A
  byte-order mark wins; otherwise valid UTF-8 is read as UTF-8 and anything
  else as Windows-1252. No dependencies are added for this.
- **From row** skips leading rows, such as a bank's preamble.
- **First row has titles** takes the first imported row as the column names
  instead of data. Without it the columns are unnamed and an application
  can number them.
- **Separators**: tab, comma, semicolon and space, any other characters, and
  *merge delimiters* for space-aligned columns. On open, the separator is
  guessed from the first 64 KiB. Space-aligned files are not guessed: tick
  Space and Merge delimiters for them.
- **String delimiter**: `"`, `'` or none. Quoted fields keep separators and
  line breaks, and a doubled quote is a literal one.
- **Trim spaces** strips spaces around unquoted fields.
- **Numbers**: you choose the decimal and thousands separators. Signs,
  accounting negatives `(15.99)` and `15.99-`, and exponents are understood.
- **Dates**: DMY, MDY or YMD, with `/ - .` or space separators, or 8 compact
  digits. A two-digit year means 1930–2029. A time after the date is ignored.
  Standard columns also accept ISO 8601 (`2026-12-31`) whatever the order.
- **Column type** applies to the column picked in the preview:
  - *Standard*: a number, else a date (with *detect special numbers*), else
    text.
  - *Text*: always text, so codes keep their leading zeros.
  - *Date (DMY/MDY/YMD)*.
  - *Hide*: the column is left out.
- **Quoted fields as text** keeps every quoted field as text.

The dialog fits an 80×25 terminal and can be resized. The preview grows with
the screen, up to 20 rows.

## Use

```toml
[dependencies]
tv-text-import = { git = "https://github.com/aaloy/tv-text-import" }
```

```rust
use tv_text_import::{TextImportDialog, Value};

let bytes = std::fs::read("movements.csv")?;
if let Some(imported) = TextImportDialog::new("movements.csv", bytes).execute(&mut app) {
    for row in &imported.rows {
        match &row[2] {
            Value::Number(amount) => { /* … */ }
            Value::Date(date) => { /* a chrono::NaiveDate */ }
            Value::Text(text) => { /* … */ }
            Value::Empty => {}
        }
    }
    // Keep imported.options to start the next import from them.
}
```

- `TextImportDialog::new(name, bytes)` starts from the defaults: UTF-8, comma,
  `"`, trim on, `.` decimals, no thousands separator, DMY.
- `with_defaults(name, bytes, &options)` starts from your options, such as
  the ones the last import returned or a European `;` and `1.234,56`, and
  still detects the character set and separator.
- `with_options(name, bytes, options)` uses your options as they are.

Without the dialog, `import(&bytes, &options)` does the same work, and
`detect_options(&bytes, &base)` makes the guesses.

### What you get back

`Imported` has three fields:

- `columns`: each kept column's position in the file, its type and its title
  (`Some` only when *First row has titles* is on and the cell is not empty).
- `rows`: one `Value` per kept column. Short rows are padded with
  `Value::Empty`.
- `options`: what was used, so the next import can start from it.

## Example

```sh
cargo run --example text_import examples/data/bank_windows1252.csv
```

It opens the dialog on the file. With no file given, press F3 to pick one.
The result appears in its own window, with the file's titles as column
headers when *First row has titles* is on and the column number otherwise.
The Window menu tiles or cascades the windows, and also zooms, cycles and
closes them. Each later import starts from the previous one's options.

The `examples/data` samples cover the cases above:

| File | Shows |
| --- | --- |
| `bank_windows1252.csv` | Windows-1252, `;`, `1.234,56`, DMY |
| `us_sales.csv` | `,`, `1,234.50`, MDY, quoted commas, `""` and a line break in a field, accounting negatives |
| `cities.tsv` | Tab, UTF-8 with BOM, zip codes kept as Text, YMD dates |
| `budget_utf16.csv` | UTF-16 LE, `;`, a column to Hide |
| `aligned.txt` | Space-aligned columns: Space + Merge delimiters, and a title row |

## Requirements

It depends on the `integration/all` branch of
[aaloy/turbo-vision-4-rust](https://github.com/aaloy/turbo-vision-4-rust). That
branch has `Form`, `GroupBox`, frozen table panes and Unicode text input,
which are not yet in a crates.io release. Once they are released, the
dependency will move to crates.io.

## Not yet

- Fixed-width files (column breaks set by clicking in the preview).
- Detecting the decimal and thousands separators from the data. Set them,
  or pass defaults that suit your users.

## License

MIT
