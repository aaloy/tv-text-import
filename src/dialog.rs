//! The Text Import dialog.
//!
//! The dialog is built with `Form`, so it sizes and centres itself and its
//! labels focus their controls with Alt + letter. While it is open, the tick
//! of `Application::execute_modal` reads the controls after every event and,
//! when an option changed, splits the first rows again and refreshes the
//! preview. In the preview, Left and Right pick a column; "Column type" then
//! shows and sets that column's type, which the preview's header shows.

use turbo_vision::app::{Application, ModalTick};
use turbo_vision::core::command::{CM_OK, CM_USER, CommandId};
use turbo_vision::views::checkbox::CheckBox;
use turbo_vision::views::combo_box::ComboBox;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::form::{Form, size};
use turbo_vision::views::handle::Handle;
use turbo_vision::views::input_line::InputLine;
use turbo_vision::views::spinner::Spinner;
use turbo_vision::views::table::{Column, Table};
use turbo_vision::views::{GroupLike, View};

use crate::charset::Charset;
use crate::import::{Imported, detect_options, import};
use crate::options::{ColumnType, DateOrder, ImportOptions, Separators};
use crate::split::{Field, split};

/// The first `ComboBox` id the dialog uses; it takes this one and the five
/// after it. Combo box ids must be unique among the combo boxes alive at the
/// same time.
pub const FIRST_COMBO_ID: u16 = 0xE100;

/// The command the preview table sends on Enter or a double click. The
/// dialog ignores it.
const CM_PREVIEW_PICK: CommandId = CM_USER + 0x7E00;

/// How many rows the preview splits and shows.
const PREVIEW_ROWS: usize = 100;

/// The preview's height in rows, header included: what the screen leaves,
/// within these bounds.
const PREVIEW_HEIGHT: (i16, i16) = (4, 20);

/// The widest a preview column is drawn.
const PREVIEW_COLUMN_WIDTH: usize = 24;

/// String delimiters offered, and how the dialog lists them.
const QUOTES: [(Option<char>, &str); 3] = [(Some('"'), "\""), (Some('\''), "'"), (None, "none")];

/// Decimal separators offered.
const DECIMALS: [(char, &str); 2] = [('.', "Point ."), (',', "Comma ,")];

/// Thousands separators offered.
const THOUSANDS: [(Option<char>, &str); 5] = [
    (None, "None"),
    (Some(','), "Comma ,"),
    (Some('.'), "Point ."),
    (Some(' '), "Space"),
    (Some('\''), "Apostrophe '"),
];

/// A Text Import dialog over the bytes of one file.
pub struct TextImportDialog {
    title: String,
    bytes: Vec<u8>,
    options: ImportOptions,
}

impl TextImportDialog {
    /// A dialog for `bytes`, titled after `name` (a file name, usually).
    /// The character set and separator are guessed; the rest starts from
    /// [`ImportOptions::default`].
    #[must_use]
    pub fn new(name: &str, bytes: Vec<u8>) -> Self {
        Self::with_defaults(name, bytes, &ImportOptions::default())
    }

    /// Like [`new`](Self::new), but the options start from `defaults` (the
    /// user's usual decimal separator, say) before the character set and
    /// separator are guessed.
    #[must_use]
    pub fn with_defaults(name: &str, bytes: Vec<u8>, defaults: &ImportOptions) -> Self {
        let options = detect_options(&bytes, defaults);
        Self {
            title: format!("Text Import - {name}"),
            bytes,
            options,
        }
    }

    /// A dialog that starts from exactly `options`, guessing nothing: the
    /// options an earlier import of the same kind of file returned.
    #[must_use]
    pub fn with_options(name: &str, bytes: Vec<u8>, options: ImportOptions) -> Self {
        Self {
            title: format!("Text Import - {name}"),
            bytes,
            options,
        }
    }

    /// The options the dialog opens with.
    #[must_use]
    pub fn options(&self) -> &ImportOptions {
        &self.options
    }

    /// Shows the dialog. Returns the imported rows on OK, `None` on Cancel or
    /// Esc.
    pub fn execute(self, app: &mut Application) -> Option<Imported> {
        let (mut dialog, controls) = build_to_fit(&self.title, app.desktop.bounds().height());
        dialog.prepare_modal(app);
        let mut live = Live::new(&self.bytes, self.options);
        live.show(&mut dialog, &controls);
        let bytes = &self.bytes;
        let command = app.execute_modal(&mut dialog, |_, dialog| {
            live.sync(dialog, &controls, bytes);
            ModalTick::Continue
        });
        if command != CM_OK {
            return None;
        }
        live.sync(&mut dialog, &controls, bytes);
        Some(import(bytes, &live.options))
    }
}

/// The handles of the dialog's controls.
struct Controls {
    charset: Handle<ComboBox>,
    from_row: Handle<Spinner>,
    first_row_titles: Handle<CheckBox>,
    tab: Handle<CheckBox>,
    comma: Handle<CheckBox>,
    semicolon: Handle<CheckBox>,
    space: Handle<CheckBox>,
    other: Handle<InputLine>,
    merge: Handle<CheckBox>,
    trim: Handle<CheckBox>,
    quote: Handle<ComboBox>,
    quoted_as_text: Handle<CheckBox>,
    detect_special_numbers: Handle<CheckBox>,
    decimal: Handle<ComboBox>,
    thousands: Handle<ComboBox>,
    date_order: Handle<ComboBox>,
    column_type: Handle<ComboBox>,
    preview: Handle<Table>,
}

/// A check box just wide enough for its label.
fn check_box(label: &str) -> CheckBox {
    let width = label.chars().filter(|&c| c != '~').count() + 4;
    CheckBox::new(size(i16::try_from(width).unwrap_or(i16::MAX), 1), label)
}

/// A combo box `width` columns wide listing `items`.
fn combo<'a>(width: i16, id: u16, items: impl IntoIterator<Item = &'a str>) -> ComboBox {
    ComboBox::with_items(
        size(width, 1),
        id,
        items.into_iter().map(String::from).collect(),
    )
}

/// Lays the dialog out. Hot keys: Ch~a~racter set, ~F~rom row, ~T~ab,
/// ~C~omma, ~S~emicolon, S~p~ace, Ot~h~er, ~M~erge, T~r~im, String
/// ~d~elimiter, ~q~uoted as text, special ~n~umbers, Dec~i~mal, Tho~u~sands,
/// Dat~e~s, Column t~y~pe, and ~O~K.
fn build(title: &str, preview_height: i16) -> (Dialog, Controls) {
    let mut form = Form::new(title);
    form.resizable(true);
    form.spacing(0);

    form.group("Import");
    let mut line = form.line();
    let charset = line.field(
        "Ch~a~racter set",
        combo(26, FIRST_COMBO_ID, Charset::ALL.iter().map(|c| c.label())),
    );
    let from_row = line.field("~F~rom row", Spinner::new(size(9, 1), 1, 9_999_999));
    let first_row_titles = line.field("", check_box("First row ha~s~ titles"));
    let mut line = form.line();
    let decimal = line.field(
        "Dec~i~mal",
        combo(11, FIRST_COMBO_ID + 2, DECIMALS.iter().map(|d| d.1)),
    );
    let thousands = line.field(
        "Tho~u~sands",
        combo(16, FIRST_COMBO_ID + 3, THOUSANDS.iter().map(|t| t.1)),
    );
    let date_order = line.field(
        "Dat~e~s",
        combo(
            7,
            FIRST_COMBO_ID + 4,
            DateOrder::ALL.iter().map(|o| o.label()),
        ),
    );
    let mut line = form.line();
    let quoted_as_text = line.field("", check_box("~Q~uoted fields as text"));
    let detect_special_numbers = line.field("", check_box("Detect special ~n~umbers"));
    form.end_group();

    form.group("Separator options");
    let mut line = form.line();
    let tab = line.field("", check_box("~T~ab"));
    let comma = line.field("", check_box("~C~omma"));
    let semicolon = line.field("", check_box("~S~emicolon"));
    let space = line.field("", check_box("S~p~ace"));
    let other = line.field("Ot~h~er", InputLine::new(size(6, 1), 8));
    let mut line = form.line();
    let merge = line.field("", check_box("~M~erge delimiters"));
    let trim = line.field("", check_box("T~r~im spaces"));
    let quote = line.field(
        "String ~d~elimiter",
        combo(8, FIRST_COMBO_ID + 1, QUOTES.iter().map(|q| q.1)),
    );
    form.end_group();

    form.group("Fields");
    let column_type = form.field(
        "Column t~y~pe",
        combo(
            14,
            FIRST_COMBO_ID + 5,
            ColumnType::ALL.iter().map(|t| t.label()),
        ),
    );
    let mut table = Table::new(size(0, preview_height), CM_PREVIEW_PICK);
    table.set_separators(true);
    table.set_frozen_cols(1); // the row numbers stay while the fields scroll
    let preview = form.row(table);
    form.end_group();

    form.ok_cancel();
    let dialog = form.build();
    let controls = Controls {
        charset,
        from_row,
        first_row_titles,
        tab,
        comma,
        semicolon,
        space,
        other,
        merge,
        trim,
        quote,
        quoted_as_text,
        detect_special_numbers,
        decimal,
        thousands,
        date_order,
        column_type,
        preview,
    };
    (dialog, controls)
}

/// The dialog with the tallest preview that lets it fit in `height` rows.
fn build_to_fit(title: &str, height: i16) -> (Dialog, Controls) {
    let (min, max) = PREVIEW_HEIGHT;
    let fixed = build(title, 1).0.bounds().height() - 1;
    build(title, (height - fixed).clamp(min, max))
}

/// The dialog's state between events: the options last read, the decoded
/// text and the split preview rows.
struct Live {
    options: ImportOptions,
    text: String,
    rows: Vec<Vec<Field>>,
    /// The field column picked in the preview (from 0, not counting the
    /// row-number column).
    column: usize,
    /// What the "Column type" combo box showed when last read.
    column_type: Option<usize>,
}

impl Live {
    fn new(bytes: &[u8], options: ImportOptions) -> Self {
        let text = options.charset.decode(bytes);
        let rows = split(&text, &options, Some(PREVIEW_ROWS));
        Self {
            options,
            text,
            rows,
            column: 0,
            column_type: None,
        }
    }

    /// Puts the options into the controls and fills the preview.
    fn show(&mut self, dialog: &mut Dialog, c: &Controls) {
        let o = &self.options;
        set_combo(
            dialog,
            c.charset,
            Charset::ALL.iter().position(|&x| x == o.charset),
        );
        if let Some(s) = dialog.get_mut(c.from_row) {
            s.set_value(i64::try_from(o.from_row).unwrap_or(1));
        }
        let checks = [
            (c.first_row_titles, o.first_row_titles),
            (c.tab, o.separators.tab),
            (c.comma, o.separators.comma),
            (c.semicolon, o.separators.semicolon),
            (c.space, o.separators.space),
            (c.merge, o.separators.merge),
            (c.trim, o.trim),
            (c.quoted_as_text, o.quoted_as_text),
            (c.detect_special_numbers, o.detect_special_numbers),
        ];
        for (handle, on) in checks {
            if let Some(b) = dialog.get_mut(handle) {
                b.set_checked(on);
            }
        }
        if let Some(i) = dialog.get_mut(c.other) {
            i.set_text(o.separators.other.clone());
        }
        set_combo(dialog, c.quote, QUOTES.iter().position(|q| q.0 == o.quote));
        set_combo(
            dialog,
            c.decimal,
            DECIMALS.iter().position(|d| d.0 == o.decimal).or(Some(0)),
        );
        set_combo(
            dialog,
            c.thousands,
            THOUSANDS
                .iter()
                .position(|t| t.0 == o.thousands)
                .or(Some(0)),
        );
        set_combo(
            dialog,
            c.date_order,
            DateOrder::ALL.iter().position(|&d| d == o.date_order),
        );
        self.column_type = ColumnType::ALL
            .iter()
            .position(|&t| t == o.column_type(self.column));
        set_combo(dialog, c.column_type, self.column_type);
        self.fill_preview(dialog, c);
    }

    /// Reads the controls; when anything changed, updates the preview.
    fn sync(&mut self, dialog: &mut Dialog, c: &Controls, bytes: &[u8]) {
        let mut read = read_options(dialog, c, &self.options);

        // The column picked in the preview, and its type.
        let picked = dialog.get(c.preview).map_or(1, Table::selected_col);
        let picked = picked.max(1) - 1;
        let shown_type = dialog.get(c.column_type).and_then(ComboBox::selected);
        if picked != self.column {
            self.column = picked;
            self.column_type = ColumnType::ALL
                .iter()
                .position(|&t| t == read.column_type(picked));
            set_combo(dialog, c.column_type, self.column_type);
        } else if shown_type != self.column_type {
            self.column_type = shown_type;
            if let Some(t) = shown_type.and_then(|i| ColumnType::ALL.get(i)) {
                read.set_column_type(picked, *t);
            }
        }

        if read == self.options {
            return;
        }
        let resplit = read.charset != self.options.charset
            || read.from_row != self.options.from_row
            || read.separators != self.options.separators
            || read.quote != self.options.quote
            || read.trim != self.options.trim;
        if read.charset != self.options.charset {
            self.text = read.charset.decode(bytes);
        }
        if resplit {
            self.rows = split(&self.text, &read, Some(PREVIEW_ROWS));
        }
        self.options = read;
        self.fill_preview(dialog, c);
    }

    /// Shows the split rows in the preview: a row-number column, then one
    /// column per field titled with its type.
    fn fill_preview(&self, dialog: &mut Dialog, c: &Controls) {
        let width = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        let first = self.options.from_row;
        let last_number = first + self.rows.len().saturating_sub(1);
        let number_width = last_number.to_string().len();

        let mut columns = vec![Column::right("", to_u16(number_width))];
        for col in 0..width {
            let title = self.options.column_type(col).label();
            let widest = self
                .rows
                .iter()
                .filter_map(|r| r.get(col))
                .map(|f| shown(&f.text).chars().count())
                .max()
                .unwrap_or(0);
            let w = widest
                .max(title.chars().count())
                .clamp(3, PREVIEW_COLUMN_WIDTH);
            columns.push(Column::new(title, to_u16(w)));
        }
        let rows = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut cells = Vec::with_capacity(width + 1);
                let number = if self.options.first_row_titles && i == 0 {
                    1
                } else {
                    first + i
                };
                cells.push(number.to_string());
                cells.extend(
                    (0..width).map(|col| r.get(col).map(|f| shown(&f.text)).unwrap_or_default()),
                );
                cells
            })
            .collect();

        if let Some(table) = dialog.get_mut(c.preview) {
            let row = table.selected_row();
            table.set_columns(columns);
            table.set_rows(rows);
            table.set_selected_col((self.column + 1).min(width.max(1)));
            if let Some(row) = row {
                table.set_selected_row(row);
            }
        }
    }
}

/// The options the controls show. Column types are not controls of their
/// own; they come from `previous`.
fn read_options(dialog: &Dialog, c: &Controls, previous: &ImportOptions) -> ImportOptions {
    let checked = |h: Handle<CheckBox>| dialog.get(h).is_some_and(CheckBox::is_checked);
    let picked = |h: Handle<ComboBox>| dialog.get(h).and_then(ComboBox::selected);
    let from_row = dialog.get(c.from_row).map_or(1, Spinner::value);
    ImportOptions {
        charset: picked(c.charset)
            .and_then(|i| Charset::ALL.get(i).copied())
            .unwrap_or(previous.charset),
        from_row: usize::try_from(from_row).unwrap_or(1).max(1),
        first_row_titles: checked(c.first_row_titles),
        separators: Separators {
            tab: checked(c.tab),
            comma: checked(c.comma),
            semicolon: checked(c.semicolon),
            space: checked(c.space),
            other: dialog
                .get(c.other)
                .map(|i| i.text().to_string())
                .unwrap_or_default(),
            merge: checked(c.merge),
        },
        quote: picked(c.quote)
            .and_then(|i| QUOTES.get(i))
            .map_or(previous.quote, |q| q.0),
        trim: checked(c.trim),
        quoted_as_text: checked(c.quoted_as_text),
        detect_special_numbers: checked(c.detect_special_numbers),
        decimal: picked(c.decimal)
            .and_then(|i| DECIMALS.get(i))
            .map_or(previous.decimal, |d| d.0),
        thousands: picked(c.thousands)
            .and_then(|i| THOUSANDS.get(i))
            .map_or(previous.thousands, |t| t.0),
        date_order: picked(c.date_order)
            .and_then(|i| DateOrder::ALL.get(i).copied())
            .unwrap_or(previous.date_order),
        columns: previous.columns.clone(),
    }
}

fn set_combo(dialog: &mut Dialog, handle: Handle<ComboBox>, index: Option<usize>) {
    if let Some(combo) = dialog.get_mut(handle) {
        combo.set_selected(index);
    }
}

/// A field as the preview shows it: on one line.
fn shown(text: &str) -> String {
    text.replace("\r\n", "↵").replace(['\r', '\n'], "↵")
}

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANK: &[u8] = b"Fecha;Concepto;Importe\n02/10/2026;COMPRA \"PE\xD1A\";-45,20\n03/10/2026;NOMINA;1.500,00\n";

    fn opened() -> (Dialog, Controls, Live) {
        let (mut dialog, controls) = build("Text Import - test", 8);
        let mut live = Live::new(BANK, detect_options(BANK, &ImportOptions::default()));
        live.show(&mut dialog, &controls);
        (dialog, controls, live)
    }

    /// The preview's column titles and cells, read through its selection
    /// (which is put back afterwards).
    fn preview(dialog: &mut Dialog, c: &Controls) -> (Vec<String>, Vec<Vec<String>>) {
        let table = dialog.get_mut(c.preview).unwrap();
        let (row, col) = (table.selected_row(), table.selected_col());
        let titles = table
            .columns()
            .iter()
            .map(|column| column.title.clone())
            .collect();
        let width = table.columns().len();
        let rows = (0..table.row_count())
            .map(|r| {
                table.set_selected_row(r);
                (0..width)
                    .map(|k| {
                        table.set_selected_col(k);
                        table.selected_cell().unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        if let Some(row) = row {
            table.set_selected_row(row);
        }
        table.set_selected_col(col);
        (titles, rows)
    }

    #[test]
    fn opens_with_the_guessed_options_in_its_controls() {
        let (dialog, c, live) = opened();
        assert!(dialog.get(c.semicolon).unwrap().is_checked());
        assert!(!dialog.get(c.comma).unwrap().is_checked());
        assert_eq!(dialog.get(c.charset).unwrap().selected(), Some(3)); // Windows-1252
        assert_eq!(read_options(&dialog, &c, &live.options), live.options);
    }

    #[test]
    fn the_preview_shows_row_numbers_and_types_over_split_fields() {
        let (mut dialog, c, _) = opened();
        let (titles, rows) = preview(&mut dialog, &c);
        assert_eq!(titles, vec!["", "Standard", "Standard", "Standard"]);
        assert_eq!(
            rows[1],
            vec!["2", "02/10/2026", "COMPRA \"PEÑA\"", "-45,20"]
        );
    }

    #[test]
    fn changing_a_separator_splits_again() {
        let (mut dialog, c, mut live) = opened();
        dialog.get_mut(c.semicolon).unwrap().set_checked(false);
        dialog.get_mut(c.comma).unwrap().set_checked(true);
        live.sync(&mut dialog, &c, BANK);
        let (_, rows) = preview(&mut dialog, &c);
        assert_eq!(rows[1], vec!["2", "02/10/2026;COMPRA \"PEÑA\";-45", "20"]);
    }

    #[test]
    fn changing_the_charset_decodes_again() {
        let (mut dialog, c, mut live) = opened();
        set_combo(&mut dialog, c.charset, Some(0)); // UTF-8: the Latin-1 Ñ is not valid
        live.sync(&mut dialog, &c, BANK);
        assert_eq!(preview(&mut dialog, &c).1[1][2], "COMPRA \"PE\u{FFFD}A\"");
    }

    #[test]
    fn from_row_skips_rows_and_keeps_their_numbers() {
        let (mut dialog, c, mut live) = opened();
        dialog.get_mut(c.from_row).unwrap().set_value(2);
        live.sync(&mut dialog, &c, BANK);
        let (_, rows) = preview(&mut dialog, &c);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], "2");
    }

    #[test]
    fn a_column_type_follows_the_picked_column() {
        let (mut dialog, c, mut live) = opened();
        dialog.get_mut(c.preview).unwrap().set_selected_col(1);
        live.sync(&mut dialog, &c, BANK);
        set_combo(&mut dialog, c.column_type, Some(2)); // Date (DMY)
        live.sync(&mut dialog, &c, BANK);
        assert_eq!(
            live.options.column_type(0),
            ColumnType::Date(DateOrder::Dmy)
        );
        assert_eq!(preview(&mut dialog, &c).0[1], "Date (DMY)");

        // Picking another column shows that column's type.
        dialog.get_mut(c.preview).unwrap().set_selected_col(3);
        live.sync(&mut dialog, &c, BANK);
        assert_eq!(dialog.get(c.column_type).unwrap().selected(), Some(0));
        set_combo(&mut dialog, c.column_type, Some(5)); // Hide
        live.sync(&mut dialog, &c, BANK);
        assert_eq!(live.options.column_type(2), ColumnType::Hide);
        assert_eq!(preview(&mut dialog, &c).0[3], "Hide");
    }

    #[test]
    fn the_result_is_typed_with_the_dialogs_options() {
        let (mut dialog, c, mut live) = opened();
        dialog.get_mut(c.from_row).unwrap().set_value(2);
        set_combo(&mut dialog, c.decimal, Some(1));
        set_combo(&mut dialog, c.thousands, Some(2));
        live.sync(&mut dialog, &c, BANK);
        let imported = import(BANK, &live.options);
        assert_eq!(imported.rows[1][2], crate::Value::Number(1500.0));
        assert_eq!(
            imported.rows[0][1],
            crate::Value::Text("COMPRA \"PEÑA\"".into())
        );
    }
}

#[cfg(test)]
mod screen_probe {
    use super::*;

    #[test]
    #[ignore = "prints the dialog; run with --ignored --nocapture"]
    fn print_the_dialog() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/data/bank_windows1252.csv"
        ))
        .unwrap();
        let (mut dialog, controls) = build_to_fit("Text Import - bank_windows1252.csv", 23);
        let mut live = Live::new(&bytes, detect_options(&bytes, &ImportOptions::default()));
        live.show(&mut dialog, &controls);
        let mut terminal = turbo_vision::test_util::test_terminal(80, 25);
        let b = dialog.bounds();
        println!("dialog bounds {:?}", b);
        dialog.draw(&mut terminal);
        for row in terminal
            .buffer()
            .iter()
            .take(usize::try_from(b.b.y + 1).unwrap())
        {
            let line: String = row.iter().map(|c| c.ch).collect();
            println!("{}", line.trim_end());
        }
    }
}
