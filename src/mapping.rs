//! Mapping imported columns to the fields of a database table (or any other
//! target): what the application wants, which column feeds each field, and the
//! rows rearranged to match.

use crate::import::Imported;
use crate::transform::{Transform, apply_all};
use crate::value::Value;

/// One field the application wants filled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetField {
    /// The field's name, such as the database column's.
    pub name: String,
    /// What the dialog shows for it; the name when `None`.
    pub label: Option<String>,
    /// An import without a column for this field makes no sense.
    pub required: bool,
}

impl TargetField {
    /// An optional field called `name`.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            label: None,
            required: false,
        }
    }

    /// Shows `label` in the dialog instead of the name.
    #[must_use]
    pub fn label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    /// Makes the field required.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// What the dialog shows for the field.
    #[must_use]
    pub fn shown(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }
}

/// Which imported column feeds each target field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// By target position: the index into [`Imported::columns`] (and into
    /// each row) feeding it, or `None` to leave the field empty.
    pub sources: Vec<Option<usize>>,
    /// By target position: the changes [`apply`](Self::apply) makes to the
    /// field's value, in order. Shorter than `sources` (or empty) means no
    /// changes for the fields past its end.
    pub transforms: Vec<Vec<Transform>>,
}

impl Mapping {
    /// A mapping of `sources` with no transforms.
    #[must_use]
    pub fn new(sources: Vec<Option<usize>>) -> Self {
        Self {
            sources,
            transforms: Vec::new(),
        }
    }

    /// A mapping with nothing mapped.
    #[must_use]
    pub fn none(targets: &[TargetField]) -> Self {
        Self::new(vec![None; targets.len()])
    }

    /// Matches columns to fields by title, ignoring case, spaces and
    /// punctuation, against the field's name and label. A file without
    /// titles is matched by position instead. A column feeds one field at
    /// most.
    #[must_use]
    pub fn auto(targets: &[TargetField], imported: &Imported) -> Self {
        let mut taken = vec![false; imported.columns.len()];
        let titled = imported.columns.iter().any(|c| c.title.is_some());
        let sources = targets
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let found = if titled {
                    let wanted = [normal(&target.name), normal(target.shown())];
                    imported.columns.iter().enumerate().position(|(k, c)| {
                        !taken[k]
                            && c.title
                                .as_deref()
                                .is_some_and(|t| wanted.contains(&normal(t)))
                    })
                } else {
                    (i < taken.len() && !taken[i]).then_some(i)
                };
                if let Some(k) = found {
                    taken[k] = true;
                }
                found
            })
            .collect();
        Self::new(sources)
    }

    /// The required fields nothing is mapped to.
    #[must_use]
    pub fn missing<'a>(&self, targets: &'a [TargetField]) -> Vec<&'a TargetField> {
        targets
            .iter()
            .enumerate()
            .filter(|&(i, t)| t.required && self.sources.get(i).copied().flatten().is_none())
            .map(|(_, t)| t)
            .collect()
    }

    /// The imported rows in the target's order: one value per field,
    /// [`Value::Empty`] where nothing is mapped, each after the field's
    /// [`transforms`](Self::transforms).
    #[must_use]
    pub fn apply(&self, imported: &Imported) -> Vec<Vec<Value>> {
        imported
            .rows
            .iter()
            .map(|row| {
                self.sources
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let value = s.and_then(|k| row.get(k)).cloned().unwrap_or(Value::Empty);
                        match self.transforms.get(i) {
                            Some(t) => apply_all(t, value),
                            None => value,
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

/// A name for comparing: lowercase letters and digits only.
pub(crate) fn normal(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{detect_options, import};
    use crate::options::ImportOptions;

    const DATA: &[u8] = b"Fecha;Importe;Nota\n02/10/2026;-45,20;luz\n03/10/2026;1500;nomina\n";

    fn imported(titles: bool) -> Imported {
        let mut o = detect_options(DATA, &ImportOptions::default());
        o.decimal = ',';
        o.first_row_titles = titles;
        import(DATA, &o)
    }

    fn targets() -> Vec<TargetField> {
        vec![
            TargetField::new("amount").label("Importe").required(),
            TargetField::new("booked_on").label("Fecha"),
            TargetField::new("memo"),
        ]
    }

    #[test]
    fn auto_matches_titles_by_name_or_label_ignoring_case_and_punctuation() {
        let t = vec![
            TargetField::new("IMPORTE"),
            TargetField::new("booked_on").label("fecha"),
        ];
        assert_eq!(
            Mapping::auto(&t, &imported(true)).sources,
            vec![Some(1), Some(0)]
        );
    }

    #[test]
    fn auto_without_titles_goes_by_position() {
        assert_eq!(
            Mapping::auto(&targets(), &imported(false)).sources,
            vec![Some(0), Some(1), Some(2)]
        );
    }

    #[test]
    fn a_column_feeds_one_field_and_unmatched_fields_stay_unmapped() {
        let t = vec![TargetField::new("amount"), TargetField::new("importe")];
        assert_eq!(
            Mapping::auto(&t, &imported(true)).sources,
            vec![None, Some(1)]
        );
    }

    #[test]
    fn missing_lists_unmapped_required_fields() {
        let t = targets();
        let m = Mapping::none(&t);
        assert_eq!(m.missing(&t).len(), 1);
        assert!(Mapping::auto(&t, &imported(true)).missing(&t).is_empty());
    }

    #[test]
    fn apply_reorders_columns_and_pads_unmapped_fields() {
        let i = imported(true);
        let m = Mapping::new(vec![Some(1), None, Some(0)]);
        let rows = m.apply(&i);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], i.rows[0][1]);
        assert_eq!(rows[0][1], Value::Empty);
        assert_eq!(rows[0][2], i.rows[0][0]);
    }

    #[test]
    fn apply_runs_a_fields_transforms_on_its_value() {
        let i = imported(true);
        let mut m = Mapping::new(vec![Some(2), Some(0)]);
        m.transforms = vec![vec![Transform::Upper], vec![]];
        let rows = m.apply(&i);
        assert_eq!(rows[0][0], Value::Text("LUZ".into()));
        assert_eq!(rows[0][1], i.rows[0][0]);
    }
}
