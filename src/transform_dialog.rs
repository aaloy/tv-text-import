//! The "Change values" dialogs, for a user who does not write expressions:
//! pick a field, then say whether to tidy its spaces, change its letters,
//! and which values in the file to replace by which others (`H` by
//! `Hombre`). The values the file has in that column are already listed, so
//! all that is left to type is what to import instead.

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
        out.extend(self.rest.iter().cloned());
        out
    }
}

/// The values a column has, each once, in the order they appear, up to
/// `limit`. Empty cells are left out.
#[must_use]
pub fn distinct_values(imported: &Imported, column: usize, limit: usize) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for row in &imported.rows {
        let Some(v) = row.get(column).map(ToString::to_string) else {
            continue;
        };
        if !v.is_empty() && !seen.contains(&v) {
            seen.push(v);
            if seen.len() == limit {
                break;
            }
        }
    }
    seen
}

/// The pairs the dialog lists: those already set, then the values the file
/// has that are not set yet (with nothing to import instead), to `rows`.
fn listed(current: &Simple, in_file: &[String], rows: usize) -> Vec<(String, String)> {
    let mut pairs = current.pairs.clone();
    for v in in_file {
        if pairs.len() >= rows {
            break;
        }
        if !pairs.iter().any(|(from, _)| from == v) {
            pairs.push((v.clone(), String::new()));
        }
    }
    pairs
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
        .filter(|&i| mapping.sources.get(i).copied().flatten().is_some())
        .collect();
    let Some(&first) = mapped.first() else {
        turbo_vision::views::msgbox::message_box_ok(
            app,
            "Choose a column for a field first, then change its values.",
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
    let current = Simple::from_transforms(mapping.transforms.get(field).map_or(&[], Vec::as_slice));
    let in_file = distinct_values(imported, column, ROWS);
    let Some(edited) = edit(app, targets[field].shown(), &current, &in_file) else {
        return false;
    };
    if mapping.transforms.len() < targets.len() {
        mapping.transforms.resize(targets.len(), Vec::new());
    }
    mapping.transforms[field] = edited.to_transforms();
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
    form.section("Replace values");
    form.row(turbo_vision::views::static_text::StaticText::new(
        size(0, 1),
        "When the file says       Import this instead",
    ));
    let width = i16::try_from(VALUE_WIDTH / 2).unwrap_or(20);
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
        },
    )
}

fn show(dialog: &mut Dialog, c: &Controls, simple: &Simple, in_file: &[String]) {
    if let Some(b) = dialog.get_mut(c.trim) {
        b.set_checked(simple.trim);
    }
    if let Some(b) = dialog.get_mut(c.case) {
        b.set_selected(Case::ALL.iter().position(|&k| k == simple.case));
    }
    let pairs = listed(simple, in_file, c.pairs.len());
    for (&(from, to), (f, t)) in c.pairs.iter().zip(pairs) {
        if let Some(i) = dialog.get_mut(from) {
            i.set_text(f);
        }
        if let Some(i) = dialog.get_mut(to) {
            i.set_text(t);
        }
    }
    if let Some(i) = dialog.get_mut(c.otherwise) {
        i.set_text(simple.otherwise.clone());
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
        rest: before.rest.clone(),
    }
}

/// Shows the editing dialog for `current`; `in_file` are the values the
/// column has, offered as the left-hand side. `None` on Cancel.
fn edit(
    app: &mut Application,
    shown: &str,
    current: &Simple,
    in_file: &[String],
) -> Option<Simple> {
    let (mut dialog, controls) = build(shown);
    dialog.prepare_modal(app);
    show(&mut dialog, &controls, current, in_file);
    if app.execute_modal(&mut dialog, |_, _| ModalTick::Continue) != CM_OK {
        return None;
    }
    Some(read(&dialog, &controls, current))
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
    fn the_file_values_not_yet_set_are_offered() {
        let current = Simple {
            pairs: vec![("H".into(), "Hombre".into())],
            ..Simple::default()
        };
        let listed = listed(&current, &["H".into(), "F".into(), "X".into()], 8);
        assert_eq!(
            listed,
            vec![
                ("H".into(), "Hombre".into()),
                ("F".into(), String::new()),
                ("X".into(), String::new())
            ]
        );
        assert_eq!(listed_len(&current, 2), 2);
    }

    fn listed_len(current: &Simple, rows: usize) -> usize {
        listed(current, &["H".into(), "F".into(), "X".into()], rows).len()
    }

    #[test]
    fn distinct_values_come_in_file_order_without_repeats_or_blanks() {
        let data = b"sex\nH\nF\nH\n\nX\n";
        let mut o = detect_options(data, &ImportOptions::default());
        o.first_row_titles = true;
        let i = import(data, &o);
        assert_eq!(distinct_values(&i, 0, 8), ["H", "F", "X"]);
        assert_eq!(distinct_values(&i, 0, 2), ["H", "F"]);
    }

    #[test]
    fn the_dialog_shows_and_reads_back_the_choices() {
        let (mut dialog, c) = build("Sex");
        let before = Simple {
            trim: true,
            case: Case::Lower,
            pairs: sex(),
            otherwise: "?".into(),
            rest: vec![Transform::Expr("value".into())],
        };
        show(&mut dialog, &c, &before, &["H".into(), "X".into()]);
        let read_back = read(&dialog, &c, &before);
        // Blank rows are shown for typing in; they change nothing.
        assert_eq!(read_back.to_transforms(), before.to_transforms());
        assert_eq!(read_back.rest, before.rest);
    }
}
