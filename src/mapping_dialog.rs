//! The field mapping dialog: one line per target field with a combo box
//! listing the imported columns, to run after the Text Import dialog.

use turbo_vision::app::Application;
use turbo_vision::core::command::CM_OK;
use turbo_vision::views::GroupLike;
use turbo_vision::views::combo_box::ComboBox;
use turbo_vision::views::dialog::Dialog;
use turbo_vision::views::handle::Handle;
use turbo_vision::views::msgbox::message_box_ok;
use tv_extensions::forms::{Form, size};

use crate::import::Imported;
use crate::mapping::{Mapping, TargetField};

/// The first `ComboBox` id the dialog uses; it takes one per target field.
/// Combo box ids must be unique among the combo boxes alive at the same
/// time, and these do not overlap the Text Import dialog's.
pub const FIRST_MAPPING_COMBO_ID: u16 = 0xE200;

/// The widest a column's entry in the combo box gets.
const ENTRY_WIDTH: usize = 40;

/// Asks which imported column feeds each target field.
pub struct FieldMappingDialog<'a> {
    title: String,
    targets: &'a [TargetField],
    imported: &'a Imported,
    start: Mapping,
}

impl<'a> FieldMappingDialog<'a> {
    /// A dialog for `imported` and the `targets` the application wants,
    /// starting from the columns [`Mapping::auto`] matches.
    #[must_use]
    pub fn new(title: &str, targets: &'a [TargetField], imported: &'a Imported) -> Self {
        Self::with_mapping(title, targets, imported, Mapping::auto(targets, imported))
    }

    /// Like [`new`](Self::new), starting from `start`: the mapping an earlier
    /// import of the same kind of file returned.
    #[must_use]
    pub fn with_mapping(
        title: &str,
        targets: &'a [TargetField],
        imported: &'a Imported,
        start: Mapping,
    ) -> Self {
        Self {
            title: title.to_string(),
            targets,
            imported,
            start,
        }
    }

    /// Shows the dialog. Returns the mapping on OK, `None` on Cancel or Esc.
    /// OK is refused, with a message, while a required field has no column.
    pub fn execute(self, app: &mut Application) -> Option<Mapping> {
        let (mut dialog, combos) = build(&self.title, self.targets, self.imported);
        dialog.prepare_modal(app);
        show(&mut dialog, &combos, &self.start);
        loop {
            if app.execute_modal(&mut dialog, |_, _| turbo_vision::app::ModalTick::Continue)
                != CM_OK
            {
                return None;
            }
            let mapping = read(&dialog, &combos);
            let missing = mapping.missing(self.targets);
            if missing.is_empty() {
                return Some(mapping);
            }
            let names: Vec<&str> = missing.iter().map(|t| t.shown()).collect();
            message_box_ok(app, &format!("Choose a column for: {}", names.join(", ")));
        }
    }
}

/// What a column is called in the combo box: its title or number, then its
/// first value.
fn entry(imported: &Imported, k: usize) -> String {
    let column = &imported.columns[k];
    let name = column
        .title
        .clone()
        .unwrap_or_else(|| format!("Column {}", k + 1));
    let sample = imported
        .rows
        .first()
        .and_then(|r| r.get(k))
        .map(|v| v.to_string().replace(['\r', '\n'], " "))
        .filter(|s| !s.is_empty());
    let text = match sample {
        Some(s) => format!("{name}  ({s})"),
        None => name,
    };
    text.chars().take(ENTRY_WIDTH).collect()
}

/// The dialog: a line per target field, a combo box listing "(none)" and the
/// columns on it.
fn build(
    title: &str,
    targets: &[TargetField],
    imported: &Imported,
) -> (Dialog, Vec<Handle<ComboBox>>) {
    let mut items = vec![String::from("(none)")];
    items.extend((0..imported.columns.len()).map(|k| entry(imported, k)));
    let width = i16::try_from(items.iter().map(|s| s.chars().count()).max().unwrap_or(6))
        .unwrap_or(i16::MAX);

    let mut form = Form::new(title);
    form.spacing(0);
    form.group("Map each field to a column");
    let combos = targets
        .iter()
        .zip(FIRST_MAPPING_COMBO_ID..)
        .map(|(target, id)| {
            let label = if target.required {
                format!("{} *", target.shown())
            } else {
                target.shown().to_string()
            };
            form.field(
                &label,
                ComboBox::with_items(size(width, 1), id, items.clone()),
            )
        })
        .collect();
    form.end_group();
    form.ok_cancel();
    (form.build(), combos)
}

/// Selects each combo box's column (entry 0 is "(none)").
fn show(dialog: &mut Dialog, combos: &[Handle<ComboBox>], mapping: &Mapping) {
    for (i, handle) in combos.iter().enumerate() {
        let source = mapping.sources.get(i).copied().flatten();
        if let Some(combo) = dialog.get_mut(*handle) {
            combo.set_selected(Some(source.map_or(0, |k| k + 1)));
        }
    }
}

/// The mapping the combo boxes show.
fn read(dialog: &Dialog, combos: &[Handle<ComboBox>]) -> Mapping {
    Mapping {
        sources: combos
            .iter()
            .map(|h| {
                dialog
                    .get(*h)
                    .and_then(ComboBox::selected)
                    .and_then(|i| i.checked_sub(1))
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{detect_options, import};
    use crate::options::ImportOptions;

    const DATA: &[u8] = b"Fecha;Importe\n02/10/2026;-45,20\n";

    fn imported() -> Imported {
        let mut o = detect_options(DATA, &ImportOptions::default());
        o.first_row_titles = true;
        import(DATA, &o)
    }

    #[test]
    fn the_combo_boxes_show_and_read_back_a_mapping() {
        let i = imported();
        let targets = [TargetField::new("amount"), TargetField::new("memo")];
        let (mut dialog, combos) = build("Map", &targets, &i);
        let mapping = Mapping {
            sources: vec![Some(1), None],
        };
        show(&mut dialog, &combos, &mapping);
        assert_eq!(dialog.get(combos[0]).unwrap().selected(), Some(2));
        assert_eq!(dialog.get(combos[1]).unwrap().selected(), Some(0));
        assert_eq!(read(&dialog, &combos), mapping);
    }

    #[test]
    fn an_entry_names_the_column_and_samples_its_first_value() {
        let i = imported();
        assert_eq!(entry(&i, 0), "Fecha  (2026-10-02)");
    }
}
