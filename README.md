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

## Mapping columns to database fields

Importing into a table needs a second step: which column feeds which field.
The application describes its fields, with no database dependency, and
`FieldMappingDialog` shows one line per field with a combo box of the imported
columns (title or number, and the first value as a sample):

```rust
let targets = [
    TargetField::new("id").required(),
    TargetField::new("amount").label("Amount"),
];
if let Some(mapping) = FieldMappingDialog::new("Map fields", &targets, &imported).execute(&mut app) {
    let rows = mapping.apply(&imported); // one Value per field, in `targets` order
}
```

- Columns are matched to fields on open: by title against the field's name
  and label (ignoring case and punctuation), or by position when the file has
  no titles. Pass `with_mapping` the mapping of an earlier import to start
  from that instead.
- A field can be left on "(none)" and then gets `Value::Empty`, except a
  required one: OK is refused with a message until it has a column.
- `Mapping::auto`, `missing` and `apply` work without the dialog.

## Transforming values

A field can change its value on the way in: `Mapping::transforms` holds, per
target field, a list of `Transform`s that `Mapping::apply` runs in order.

```rust
use tv_text_import::Transform;

mapping.transforms = vec![
    vec![Transform::Map {
        pairs: vec![("H".into(), "Hombre".into()), ("F".into(), "Mujer".into())],
        default: None, // an unlisted code stays as it is
    }],
    vec![Transform::Trim, Transform::Upper],
    vec![Transform::Expr(r#"if(value > 1000, "big", "small")"#.into())],
];
```

Transforms change text only; numbers, dates and empty fields pass through
untouched. `Trim`, `Upper`, `Lower`, `Replace`, `Prefix`, `Suffix` and `Map` need no scripting. `Expr` takes an
[evalexpr](https://crates.io/crates/evalexpr) expression where `value` is the
field; a failing expression leaves the value alone, and `Transform::check`
reports why. Templates save transforms as `transform = ...` lines.

The field mapping dialog has a **Change values...** button for users who do not
write code. It asks which field, then offers: remove spaces, ALL CAPITALS or
lowercase, add text before or after, and a table of "when the file says / import this instead". Anything
else a template holds (`Replace`, `Expr`) is kept when the field is edited.

## Templates

A template is the import settings and the field mapping of a kind of file
(a bank's export, say) saved under a name, so you do not set them each time.

```rust
// After an import and a mapping the user was happy with:
let template = Template::capture("Bank export", &imported, &targets, &mapping);
template.save_in(&dir)?;                       // dir/Bank export.tvt

// Later:
let templates = Template::load_all(&dir);      // sorted by name; bad files skipped
if let Some(choice) = choose_template(&mut app, "Import template", &templates) {
    // choice: Some(&Template), or None for "set the options by hand"
}
let imported = template.import(&bytes);        // no dialog at all...
let mapping = template.mapping(&targets, &imported);
// ...or TextImportDialog::with_options(name, bytes, template.options.clone())
// to let the user check it, and FieldMappingDialog::with_mapping(..., mapping).
```

- The options come back exactly, column types included.
- A mapping is stored per field name, with the column's position in the file
  and its title. In a file with titles the title is what matters, so the
  columns may move, and a column that disappears leaves its field unmapped
  (`Mapping::missing` flags it if required). A file without titles is matched
  by position.
- It is a text file of `key = value` lines (see `examples/templates/Bank.tvt`),
  with no extra dependencies. Keys that are missing keep their defaults (separators are off unless listed) and
  unknown ones are ignored, so templates survive new versions. A template
  that cannot be read is a `TemplateError` naming the line.

## Example

```sh
cargo run --example text_import examples/data/bank_windows1252.csv
```

It opens the dialog on the file. With no file given, press F3 to pick one.
The result appears in its own window, with the file's titles as column
headers when *First row has titles* is on and the column number otherwise.
With a templates directory (`TV_TEXT_IMPORT_TEMPLATES`, default `./templates`;
`examples/templates` has one) opening a file first asks which template to
use, and after the import you can save the options and mapping as a new one:

```sh
TV_TEXT_IMPORT_TEMPLATES=examples/templates cargo run --example text_import examples/data/bank_windows1252.csv
```

File > Open and map (F4) follows the import with the mapping dialog, for a demo
table of three fields (date, description, amount), which suits the bank sample. The Window menu tiles or cascades the windows, and also zooms, cycles and
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

`Form` and `GroupBox`-based layouts come from
[tv-extensions](https://github.com/aaloy/tv-extensions) (branch
`feat/forms-edit-synced` until released). The crate asks crates.io for
turbo-vision 4.1; the frozen table panes and Unicode text input it uses are
on the `integration/all` branch of
[aaloy/turbo-vision-4-rust](https://github.com/aaloy/turbo-vision-4-rust), so
an application patches `turbo-vision` with it so that one copy is shared:

```toml
[patch.crates-io]
turbo-vision = { git = "https://github.com/aaloy/turbo-vision-4-rust", branch = "integration/all" }
```

## Not yet

- Fixed-width files (column breaks set by clicking in the preview).
- Detecting the decimal and thousands separators from the data. Set them,
  or pass defaults that suit your users.

## License

MIT
