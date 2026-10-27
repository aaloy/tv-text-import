//! The "Change values" dialogs, for a user who does not write expressions:
//! pick a field, then say whether to tidy its spaces, change its letters,
//! and which values in the file to replace by which others (`H` by
//! `Hombre`), or add text before or after. Only text values change.

use turbo_vision::app::{Application, ModalTick};
use turbo_vision::core::command::CM_OK;
use turbo_vision::views::GroupLike;
use turbo_vision::views::checkbox::CheckBox;
use turbo_vision::views::combo_box::ComboBox;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::handle::Handle;
use turbo_vision::views::input_line::InputLine;
use tv_extensions::forms::{Form, size};

use crate::import::Imported;
use crate::mapping::{Mapping, TargetField};
use crate::transform::Transform;
use crate::value::Value;

/// The `ComboBox` id of the letters choice.
pub const CASE_COMBO_ID: u16 = 0xE400;
/// The `ComboBox` id of the field chooser.
pub const FIELD_COMBO_ID: u16 = 0xE401;

/// How many value pairs the dialog shows.
const ROWS: usize = 8;
/// The widest a value can be typed.
const VALUE_WIDTH: usize = 40;

/// What a user can ask of a field, as plain choices. The transforms it does
/// not cover (replacing text, expressions) are kept in `rest`, so editing
/// a field loaded from a template does not lose them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Simple {
    /// Remove the spaces around the text.
    pub trim: bool,
    /// Capitals, lowercase or neither.
    pub case: Case,
    /// What the file has, and what to import instead.
    pub pairs: Vec<(String, String)>,
    /// What any other value becomes; blank leaves it as it is.
    pub otherwise: String,
    /// Text put before the value.
    pub prefix: String,
    /// Text put after the value.
    pub suffix: String,
    /// Transforms the dialog does not edit.
    pub rest: Vec<Transform>,
}

/// The letters choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Case {
    /// Leave the letters as they are.
    #[default]
    Keep,
    /// ALL CAPITALS.
    Upper,
    /// all lowercase.
    Lower,
}

impl Case {
    const ALL: [Case; 3] = [Case::Keep, Case::Upper, Case::Lower];

    fn label(self) -> &'static str {
        match self {
            Case::Keep => "Leave as they are",
            Case::Upper => "ALL CAPITALS",
            Case::Lower => "all lowercase",
        }
    }
}

impl Simple {
    /// Reads a field's transforms: the first of each kind the dialog edits;
    /// the rest are kept as they are.
    #[must_use]
    pub fn from_transforms(transforms: &[Transform]) -> Self {
        let mut s = Simple::default();
        let mut have_map = false;
        for t in transforms {
            match t {
                Transform::Trim if !s.trim => s.trim = true,
                Transform::Upper | Transform::Lower if s.case == Case::Keep => {
                    s.case = if *t == Transform::Upper {
                        Case::Upper
                    } else {
                        Case::Lower
                    };
                }
                Transform::Map { pairs, default } if !have_map => {
                    have_map = true;
                    s.pairs.clone_from(pairs);
                    s.otherwise = default.clone().unwrap_or_default();
                }
                Transform::Prefix(p) if s.prefix.is_empty() => s.prefix.clone_from(p),
                Transform::Suffix(p) if s.suffix.is_empty() => s.suffix.clone_from(p),
                other => s.rest.push(other.clone()),
            }
        }
        s
    }

    /// The transforms to store: spaces, letters, the value table, then the
    /// ones kept from before. A pair with nothing on either side is
    /// dropped, so a listed value left blank stays as the file has it.
    #[must_use]
    pub fn to_transforms(&self) -> Vec<Transform> {
        let mut out = Vec::new();
        if self.trim {
            out.push(Transform::Trim);
        }
        match self.case {
            Case::Keep => {}
            Case::Upper => out.push(Transform::Upper),
            Case::Lower => out.push(Transform::Lower),
        }
        let pairs: Vec<_> = self
            .pairs
            .iter()
            .filter(|(from, to)| !from.is_empty() && !to.is_empty())
            .cloned()
            .collect();
        if !pairs.is_empty() || !self.otherwise.is_empty() {
            out.push(Transform::Map {
                pairs,
                default: (!self.otherwise.is_empty()).then(|| self.otherwise.clone()),
            });
        }
        if !self.prefix.is_empty() {
            out.push(Transform::Prefix(self.prefix.clone()));
        }
        if !self.suffix.is_empty() {
            out.push(Transform::Suffix(self.suffix.clone()));
        }
        out.extend(self.rest.iter().cloned());
        out
    }
}

/// What a column holds, which decides the changes it is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Some of it is a number: a formula can change it.
    Number,
    /// Text, with no numbers: spaces, letters, prefix, suffix, value table.
    Text,
    /// Dates or nothing: there is nothing to change.
    Other,
}

/// The [`Kind`] of `column`. A column with a few text cells among numbers
/// (`n/a`) is still a number column.
#[must_use]
pub fn column_kind(imported: &Imported, column: usize) -> Kind {
    let mut text = false;
    for v in imported.rows.iter().filter_map(|r| r.get(column)) {
        match v {
            Value::Number(_) => return Kind::Number,
            Value::Text(_) => text = true,
            _ => {}
        }
    }
    if text { Kind::Text } else { Kind::Other }
}

/// The formula of a number field and the transforms the dialog leaves be.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Formula {
    /// The formula, such as `value * -1`; blank for none.
    pub expr: String,
    /// Transforms the dialog does not edit.
    pub rest: Vec<Transform>,
}

impl Formula {
    /// Reads a field's transforms: the first formula; the rest are kept.
    #[must_use]
    pub fn from_transforms(transforms: &[Transform]) -> Self {
        let mut f = Formula::default();
        let mut have = false;
        for t in transforms {
            match t {
                Transform::Expr(e) if !have => {
                    have = true;
                    f.expr.clone_from(e);
                }
                other => f.rest.push(other.clone()),
            }
        }
        f
    }

    /// The transforms to store: the formula, then the ones kept.
    #[must_use]
    pub fn to_transforms(&self) -> Vec<Transform> {
        let mut out = Vec::new();
        if !self.expr.trim().is_empty() {
            out.push(Transform::Expr(self.expr.trim().to_string()));
        }
        out.extend(self.rest.iter().cloned());
        out
    }
}

/// Asks what to change in the fields of `mapping`: first which field (when
/// more than one is mapped), then the changes. Returns `true` when the user
/// changed something and pressed OK.
pub fn change_values(
    app: &mut Application,
    targets: &[TargetField],
    imported: &Imported,
    mapping: &mut Mapping,
) -> bool {
    let mapped: Vec<usize> = (0..targets.len())
        .filter(|&i| {
            mapping
                .sources
                .get(i)
                .copied()
                .flatten()
                .is_some_and(|k| column_kind(imported, k) != Kind::Other)
        })
        .collect();
    let Some(&first) = mapped.first() else {
        turbo_vision::views::msgbox::message_box_ok(
            app,
            "Only text and numbers can be changed. Choose a column with text or numbers for a field first.",
        );
        return false;
    };
    let field = if mapped.len() == 1 {
        first
    } else {
        match choose_field(app, targets, mapping, &mapped) {
            Some(i) => i,
            None => return false,
        }
    };
    let Some(column) = mapping.sources[field] else {
        return false;
    };
    let before = mapping.transforms.get(field).map_or(&[][..], Vec::as_slice);
    let shown = targets[field].shown();
    let edited = match column_kind(imported, column) {
        Kind::Number => {
            edit_formula(app, shown, &Formula::from_transforms(before)).map(|f| f.to_transforms())
        }
        _ => edit(app, shown, &Simple::from_transforms(before)).map(|s| s.to_transforms()),
    };
    let Some(edited) = edited else {
        return false;
    };
    if mapping.transforms.len() < targets.len() {
        mapping.transforms.resize(targets.len(), Vec::new());
    }
    mapping.transforms[field] = edited;
    true
}

/// "Which field?", marking the ones that already change their values.
fn choose_field(
    app: &mut Application,
    targets: &[TargetField],
    mapping: &Mapping,
    mapped: &[usize],
) -> Option<usize> {
    let items: Vec<String> = mapped
        .iter()
        .map(|&i| {
            let changed = mapping.transforms.get(i).is_some_and(|t| !t.is_empty());
            format!(
                "{}{}",
                targets[i].shown(),
                if changed { "  (changed)" } else { "" }
            )
        })
        .collect();
    let width = i16::try_from(items.iter().map(|s| s.chars().count()).max().unwrap_or(10))
        .unwrap_or(i16::MAX);
    let mut form = Form::new("Change values");
    let combo = form.field(
        "~F~ield",
        ComboBox::with_items(size(width, 1), FIELD_COMBO_ID, items),
    );
    form.ok_cancel();
    let mut dialog = form.build();
    dialog.prepare_modal(app);
    if let Some(c) = dialog.get_mut(combo) {
        c.set_selected(Some(0));
    }
    if app.execute_modal(&mut dialog, |_, _| ModalTick::Continue) != CM_OK {
        return None;
    }
    let picked = dialog.get(combo).and_then(ComboBox::selected)?;
    mapped.get(picked).copied()
}

/// The handles of the editing dialog's controls.
struct Controls {
    trim: Handle<CheckBox>,
    case: Handle<ComboBox>,
    pairs: Vec<(Handle<InputLine>, Handle<InputLine>)>,
    otherwise: Handle<InputLine>,
    prefix: Handle<InputLine>,
    suffix: Handle<InputLine>,
}

fn build(shown: &str) -> (Dialog, Controls) {
    let mut form = Form::new(&format!("Change values - {shown}"));
    form.spacing(0);
    let trim = form.row(CheckBox::new(size(34, 1), "Remove spaces before and after"));
    let case = form.field(
        "~L~etters",
        ComboBox::with_items(
            size(20, 1),
            CASE_COMBO_ID,
            Case::ALL.iter().map(|c| c.label().to_string()).collect(),
        ),
    );
    let width = i16::try_from(VALUE_WIDTH / 2).unwrap_or(20);
    let prefix = form.field("Add ~b~efore", InputLine::new(size(width, 1), VALUE_WIDTH));
    let suffix = form.field("Add a~f~ter", InputLine::new(size(width, 1), VALUE_WIDTH));
    form.section("Replace values");
    form.row(turbo_vision::views::static_text::StaticText::new(
        size(0, 1),
        "When the file says       Import this instead",
    ));
    let pairs = (0..ROWS)
        .map(|_| {
            let mut line = form.line();
            let from = line.field("", InputLine::new(size(width, 1), VALUE_WIDTH));
            let to = line.field("=>", InputLine::new(size(width, 1), VALUE_WIDTH));
            (from, to)
        })
        .collect();
    form.gap(1);
    let otherwise = form.field(
        "~A~nything else",
        InputLine::new(size(width, 1), VALUE_WIDTH),
    );
    form.row(turbo_vision::views::static_text::StaticText::new(
        size(0, 1),
        "(leave a box blank to keep the value as it is)",
    ));
    form.ok_cancel();
    (
        form.build(),
        Controls {
            trim,
            case,
            pairs,
            otherwise,
            prefix,
            suffix,
        },
    )
}

fn show(dialog: &mut Dialog, c: &Controls, simple: &Simple) {
    if let Some(b) = dialog.get_mut(c.trim) {
        b.set_checked(simple.trim);
    }
    if let Some(b) = dialog.get_mut(c.case) {
        b.set_selected(Case::ALL.iter().position(|&k| k == simple.case));
    }
    for (&(from, to), (f, t)) in c.pairs.iter().zip(&simple.pairs) {
        if let Some(i) = dialog.get_mut(from) {
            i.set_text(f.clone());
        }
        if let Some(i) = dialog.get_mut(to) {
            i.set_text(t.clone());
        }
    }
    for (h, text) in [
        (c.otherwise, &simple.otherwise),
        (c.prefix, &simple.prefix),
        (c.suffix, &simple.suffix),
    ] {
        if let Some(i) = dialog.get_mut(h) {
            i.set_text(text.clone());
        }
    }
}

/// `before` with what the dialog shows. Pairs the dialog could not show (past
/// its rows) and the transforms it does not edit are kept.
fn read(dialog: &Dialog, c: &Controls, before: &Simple) -> Simple {
    let text = |h: Handle<InputLine>| {
        dialog
            .get(h)
            .map(|i| i.text().trim().to_string())
            .unwrap_or_default()
    };
    let mut pairs: Vec<(String, String)> =
        c.pairs.iter().map(|&(f, t)| (text(f), text(t))).collect();
    // The pairs past the dialog's rows were not shown; keep them.
    pairs.extend(before.pairs.iter().skip(c.pairs.len()).cloned());
    Simple {
        trim: dialog.get(c.trim).is_some_and(CheckBox::is_checked),
        case: dialog
            .get(c.case)
            .and_then(ComboBox::selected)
            .and_then(|i| Case::ALL.get(i).copied())
            .unwrap_or_default(),
        pairs,
        otherwise: text(c.otherwise),
        prefix: dialog
            .get(c.prefix)
            .map(|i| i.text().to_string())
            .unwrap_or_default(),
        suffix: dialog
            .get(c.suffix)
            .map(|i| i.text().to_string())
            .unwrap_or_default(),
        rest: before.rest.clone(),
    }
}

/// Shows the editing dialog for `current`. `None` on Cancel.
fn edit(app: &mut Application, shown: &str, current: &Simple) -> Option<Simple> {
    let (mut dialog, controls) = build(shown);
    dialog.prepare_modal(app);
    show(&mut dialog, &controls, current);
    if app.execute_modal(&mut dialog, |_, _| ModalTick::Continue) != CM_OK {
        return None;
    }
    Some(read(&dialog, &controls, current))
}

/// The `InputLine` width of the formula.
const FORMULA_WIDTH: i16 = 36;

/// Shows the formula dialog for `current`, asking again while the formula
/// does not work. `None` on Cancel.
fn edit_formula(app: &mut Application, shown: &str, current: &Formula) -> Option<Formula> {
    let mut form = Form::new(&format!("Change values - {shown}"));
    form.spacing(0);
    let note = |t: &str| turbo_vision::views::static_text::StaticText::new(size(0, 1), t);
    form.row(note("These are numbers. Write a formula, where"));
    form.row(note("\"value\" is the number in the file. Examples:"));
    form.row(note("  value * -1      change the sign"));
    form.row(note("  value / 100     cents to euros"));
    form.row(note("  value + 21      add 21"));
    form.gap(1);
    let expr = form.field("~F~ormula", InputLine::new(size(FORMULA_WIDTH, 1), 120));
    form.row(note("(leave it blank to keep the numbers as they are)"));
    form.ok_cancel();
    let mut dialog = form.build();
    dialog.prepare_modal(app);
    if let Some(i) = dialog.get_mut(expr) {
        i.set_text(current.expr.clone());
    }
    loop {
        if app.execute_modal(&mut dialog, |_, _| ModalTick::Continue) != CM_OK {
            return None;
        }
        let text = dialog
            .get(expr)
            .map(|i| i.text().trim().to_string())
            .unwrap_or_default();
        let checked = if text.is_empty() {
            Ok(())
        } else {
            Transform::Expr(text.clone()).check()
        };
        match checked {
            Ok(()) => {
                return Some(Formula {
                    expr: text,
                    rest: current.rest.clone(),
                });
            }
            Err(why) => {
                turbo_vision::views::msgbox::message_box_ok(
                    app,
                    &format!("That formula does not work: {why}"),
                );
                dialog.end_modal(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{detect_options, import};
    use crate::options::ImportOptions;

    fn sex() -> Vec<(String, String)> {
        vec![("H".into(), "Hombre".into()), ("F".into(), "Mujer".into())]
    }

    #[test]
    fn simple_round_trips_through_transforms() {
        let s = Simple {
            trim: true,
            case: Case::Upper,
            pairs: sex(),
            otherwise: "?".into(),
            prefix: "ES".into(),
            suffix: "!".into(),
            rest: vec![Transform::Expr("value".into())],
        };
        assert_eq!(Simple::from_transforms(&s.to_transforms()), s);
    }

    #[test]
    fn blank_sides_are_dropped_and_nothing_set_means_no_transforms() {
        let mut s = Simple::default();
        assert!(s.to_transforms().is_empty());
        s.pairs = vec![("H".into(), String::new()), (String::new(), "x".into())];
        assert!(s.to_transforms().is_empty());
    }

    #[test]
    fn a_column_is_text_number_or_neither() {
        let data = b"name;qty;when\nana;5;2026-01-02\nluis;n/a;2026-01-03\n";
        let mut o = detect_options(data, &ImportOptions::default());
        o.first_row_titles = true;
        let i = import(data, &o);
        assert_eq!(column_kind(&i, 0), Kind::Text);
        assert_eq!(column_kind(&i, 1), Kind::Number);
        assert_eq!(column_kind(&i, 2), Kind::Other);
    }

    #[test]
    fn a_formula_round_trips_and_keeps_the_other_transforms() {
        let f = Formula {
            expr: "value * -1".into(),
            rest: vec![Transform::Upper],
        };
        assert_eq!(Formula::from_transforms(&f.to_transforms()), f);
        assert!(Formula::default().to_transforms().is_empty());
    }

    #[test]
    fn the_dialog_shows_and_reads_back_the_choices() {
        let (mut dialog, c) = build("Sex");
        let before = Simple {
            trim: true,
            case: Case::Lower,
            pairs: sex(),
            otherwise: "?".into(),
            prefix: "ES".into(),
            suffix: "!".into(),
            rest: vec![Transform::Expr("value".into())],
        };
        show(&mut dialog, &c, &before);
        let read_back = read(&dialog, &c, &before);
        // Blank rows are shown for typing in; they change nothing.
        assert_eq!(read_back.to_transforms(), before.to_transforms());
        assert_eq!(read_back.rest, before.rest);
    }
}
