// Text Import demo: opens a delimited text file, lets the user set how to
// read it in the Text Import dialog, and shows the typed rows it hands back.
//
//   cargo run --example text_import [FILE]
//
// With FILE the dialog opens at once; otherwise, or for another file, use
// File > Open (F3). Each result opens in its own window: numbers are right
// aligned and dates are shown as YYYY-MM-DD; the Window menu tiles or
// cascades the result windows. File > Open and map (F4) then asks which
// column feeds each field of a demo table (id, name, amount, date). The next
// import starts from the options of the last one, with the character set and
// separator detected again. Alt-X quits.
//
// Templates: when the directory in $TV_TEXT_IMPORT_TEMPLATES (default
// ./templates) holds any, opening a file first asks which to use, and after
// each import you can save the options (and, from F4, the field mapping) as a
// template. Try it with:
//
//   TV_TEXT_IMPORT_TEMPLATES=examples/templates cargo run --example text_import \
//       examples/data/bank_windows1252.csv

use std::path::{Path, PathBuf};

use turbo_vision::app::{AppHandler, Application};
use turbo_vision::core::command::{
    CM_CASCADE, CM_CLOSE, CM_NEXT, CM_OPEN, CM_PREV, CM_QUIT, CM_TILE, CM_USER, CM_ZOOM, CommandId,
};
use turbo_vision::core::event::Event;
use turbo_vision::core::geometry::Rect;
use turbo_vision::core::menu_data::{Menu, MenuItem, MenuItemBuilder};
use turbo_vision::core::state::Grow;
use turbo_vision::core::status_data::StatusItemBuilder;
use turbo_vision::views::file_dialog::FileDialog;
use turbo_vision::views::menu_bar::{MenuBar, SubMenu};
use turbo_vision::views::msgbox::{input_box, message_box_ok};
use turbo_vision::views::status_line::StatusLine;
use turbo_vision::views::table::{Column, TableBuilder};
use turbo_vision::views::window::WindowBuilder;
use turbo_vision::views::{GroupLike, View};
use tv_text_import::{
    FieldMappingDialog, ImportOptions, Imported, Mapping, TargetField, Template, TextImportDialog,
    Value, choose_template,
};

/// The widest a result column gets; longer values are cut in the table.
const MAX_COLUMN_WIDTH: usize = 30;

/// Open a file, then map its columns to the demo table's fields.
const CM_OPEN_MAPPED: CommandId = CM_USER + 1;

/// Remembers the options of the last import to start the next one from.
struct ImportApp {
    last: ImportOptions,
}

impl ImportApp {
    /// Reads `path`, runs the dialog on it and, on OK, shows the result.
    fn open(&mut self, app: &mut Application, path: &Path, mapped: bool) {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                message_box_ok(app, &format!("Cannot read {}:\n{error}", path.display()));
                return;
            }
        };
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let templates = Template::load_all(&templates_dir());
        let template = if templates.is_empty() {
            None
        } else {
            match choose_template(app, "Import template", &templates) {
                Some(template) => template,
                None => return,
            }
        };
        let dialog = match template {
            Some(t) => TextImportDialog::with_options(&name, bytes, t.options.clone()),
            None => TextImportDialog::with_defaults(&name, bytes, &self.last),
        };
        if let Some(imported) = dialog.execute(app) {
            self.last = imported.options.clone();
            let targets = demo_targets();
            let mut mapping = Mapping::none(&[]);
            if mapped {
                let start = template.map_or_else(
                    || Mapping::auto(&targets, &imported),
                    |t| t.mapping(&targets, &imported),
                );
                let title = format!("Map fields - {name}");
                let dialog = FieldMappingDialog::with_mapping(&title, &targets, &imported, start);
                match dialog.execute(app) {
                    Some(chosen) => mapping = chosen,
                    None => return,
                }
                let titles: Vec<String> = targets.iter().map(|t| t.name.clone()).collect();
                show(app, &name, &titles, &mapping.apply(&imported));
            } else {
                let titles: Vec<String> = (0..imported.columns.len())
                    .map(|i| {
                        imported.columns[i]
                            .title
                            .clone()
                            .unwrap_or_else(|| (i + 1).to_string())
                    })
                    .collect();
                show(app, &name, &titles, &imported.rows);
            }
            let targets = if mapped { &targets[..] } else { &[] };
            offer_to_save(app, template, &imported, targets, &mapping);
        }
    }
}

impl AppHandler for ImportApp {
    fn handle_command(&mut self, app: &mut Application, command: CommandId, _: &Event) -> bool {
        if command != CM_OPEN && command != CM_OPEN_MAPPED {
            return false;
        }
        if let Some(path) = pick_file(app) {
            self.open(app, &path, command == CM_OPEN_MAPPED);
        }
        true
    }
}

/// Asks for a file with the standard file dialog.
fn pick_file(app: &mut Application) -> Option<PathBuf> {
    let (width, height) = app.terminal.size();
    let (w, h) = (62.min(width), 20.min(height - 2));
    let x = (width - w) / 2;
    let y = (height - h) / 2;
    FileDialog::new(Rect::new(x, y, x + w, y + h), "Open a text file", "*", None)
        .build()
        .execute(app)
}

/// A value as the table shows it, with line breaks (kept inside quoted
/// fields) marked so the cell stays on one line.
fn cell(value: &Value) -> String {
    value
        .to_string()
        .replace("\r\n", "↵")
        .replace(['\r', '\n'], "↵")
}

/// The fields of the demo table the columns are mapped to, in place of a
/// real database table.
fn demo_targets() -> Vec<TargetField> {
    vec![
        TargetField::new("id").required(),
        TargetField::new("name").label("Name"),
        TargetField::new("amount").label("Amount"),
        TargetField::new("booked_on").label("Date"),
    ]
}

/// Where the templates are kept: `$TV_TEXT_IMPORT_TEMPLATES`, else
/// `./templates`.
fn templates_dir() -> PathBuf {
    std::env::var_os("TV_TEXT_IMPORT_TEMPLATES")
        .map_or_else(|| PathBuf::from("templates"), PathBuf::from)
}

/// Offers to keep what was just chosen (the import options and, after
/// File > Open and map, the field mapping) as a template. Esc or an empty
/// name skips it; the name of the template used is offered again, to update it.
fn offer_to_save(
    app: &mut Application,
    used: Option<&Template>,
    imported: &Imported,
    targets: &[TargetField],
    mapping: &Mapping,
) {
    let initial = used.map_or("", |t| t.name.as_str());
    let Some(name) = input_box(
        app,
        "Save as template",
        "~N~ame (Esc: don't save)",
        initial,
        40,
    ) else {
        return;
    };
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut template = Template::capture(name, imported, targets, mapping);
    if targets.is_empty() {
        // Opened without mapping: keep the links of the template it came from.
        template.links = used.map(|t| t.links.clone()).unwrap_or_default();
    }
    if let Err(error) = template.save_in(&templates_dir()) {
        message_box_ok(app, &format!("Cannot save the template:\n{error}"));
    }
}

/// Opens a window with `rows` in a table under `titles`.
fn show(app: &mut Application, name: &str, titles: &[String], rows: &[Vec<Value>]) {
    let texts: Vec<Vec<String>> = rows
        .iter()
        .map(|row| row.iter().map(cell).collect())
        .collect();
    let columns = titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let widest = texts
                .iter()
                .filter_map(|r| r.get(i))
                .map(|v| v.chars().count())
                .max()
                .unwrap_or(0)
                .max(title.chars().count())
                .clamp(3, MAX_COLUMN_WIDTH);
            let width = u16::try_from(widest).unwrap_or(u16::MAX);
            let numeric = rows
                .iter()
                .any(|r| matches!(r.get(i), Some(Value::Number(_))));
            if numeric {
                Column::right(title, width)
            } else {
                Column::new(title, width)
            }
        })
        .collect();

    let desktop = app.desktop.bounds();
    let bounds = Rect::new(1, 0, desktop.width() - 1, desktop.height() - 1);
    let title = format!("{name}: {} rows, {} columns", rows.len(), titles.len());
    let mut window = WindowBuilder::new().bounds(bounds).title(&title).build();
    let mut table = TableBuilder::new()
        .bounds(Rect::new(0, 0, bounds.width() - 2, bounds.height() - 2))
        .columns(columns)
        .rows(texts)
        .separators(true)
        .build();
    table.set_grow_mode(Grow::HI_X | Grow::HI_Y);
    window.add(table);
    window.set_initial_focus();
    app.desktop.add(window);
}

fn main() -> turbo_vision::core::error::Result<()> {
    let mut app = Application::new()?;
    let (width, height) = app.terminal.size();

    let mut menu_bar = MenuBar::new(Rect::new(0, 0, width, 1));
    menu_bar.add_submenu(SubMenu::new(
        "~F~ile",
        Menu::from_items(vec![
            MenuItemBuilder::new()
                .text("~O~pen...")
                .command(CM_OPEN)
                .key("F3")
                .build(),
            MenuItemBuilder::new()
                .text("Open and ~m~ap...")
                .command(CM_OPEN_MAPPED)
                .key("F4")
                .build(),
            MenuItem::separator(),
            MenuItemBuilder::new()
                .text("E~x~it")
                .command(CM_QUIT)
                .key("Alt+X")
                .build(),
        ]),
    ));
    menu_bar.add_submenu(SubMenu::new(
        "~W~indow",
        Menu::from_items(vec![
            MenuItemBuilder::new()
                .text("~T~ile")
                .command(CM_TILE)
                .build(),
            MenuItemBuilder::new()
                .text("C~a~scade")
                .command(CM_CASCADE)
                .build(),
            MenuItem::separator(),
            MenuItemBuilder::new()
                .text("~Z~oom")
                .command(CM_ZOOM)
                .key("F5")
                .build(),
            MenuItemBuilder::new()
                .text("~N~ext")
                .command(CM_NEXT)
                .key("F6")
                .build(),
            MenuItemBuilder::new()
                .text("~P~revious")
                .command(CM_PREV)
                .key("Shift+F6")
                .build(),
            MenuItemBuilder::new()
                .text("~C~lose")
                .command(CM_CLOSE)
                .key("Alt+F3")
                .build(),
        ]),
    ));
    app.set_menu_bar(menu_bar);
    app.set_status_line(StatusLine::new(
        Rect::new(0, height - 1, width, height),
        vec![
            StatusItemBuilder::new()
                .text("~F3~ Open")
                .key("F3")
                .command(CM_OPEN)
                .build(),
            StatusItemBuilder::new()
                .text("~F4~ Open and map")
                .key("F4")
                .command(CM_OPEN_MAPPED)
                .build(),
            StatusItemBuilder::new()
                .text("~Alt-X~ Exit")
                .key("Alt+X")
                .command(CM_QUIT)
                .build(),
        ],
    ));

    let mut handler = ImportApp {
        last: ImportOptions::default(),
    };
    if let Some(path) = std::env::args_os().nth(1) {
        handler.open(&mut app, Path::new(&path), false);
    }
    app.run_with(&mut handler);
    Ok(())
}
