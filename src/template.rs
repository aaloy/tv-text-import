//! Import templates: the import settings and the field mapping of a kind of
//! file, saved under a name so the next file of that kind needs neither.
//!
//! A template is stored as plain text, one `key = value` per line, so it
//! can be read, diffed and edited by hand:
//!
//! ```text
//! name = Bank export
//! charset = windows-1252
//! from_row = 2
//! separator.semicolon = true
//! decimal = ,
//! column.3 = hide
//! map = 2<TAB>amount<TAB>Importe
//! transform = gender<TAB>map<TAB>H<TAB>Hombre<TAB>F<TAB>Mujer
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

use crate::charset::Charset;
use crate::import::{Imported, import};
use crate::mapping::{Mapping, TargetField, normal};
use crate::options::{ColumnType, DateOrder, ImportOptions, Separators};
use crate::transform::Transform;

/// The extension of a template file.
pub const EXTENSION: &str = "tvt";

/// Which column feeds one target field, in a way that survives the next
/// file: by title when it has one, else by position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldLink {
    /// The [`TargetField::name`] it feeds.
    pub field: String,
    /// The column's position in the file, from 0, counting hidden columns.
    pub source: usize,
    /// The column's title, when the file had titles. A file with titles is
    /// matched by this alone; `source` serves a file without.
    pub title: Option<String>,
    /// The changes made to the field's value, in order.
    pub transforms: Vec<Transform>,
}

/// A named set of import options and field links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// What the user picks it by.
    pub name: String,
    /// How the file is read.
    pub options: ImportOptions,
    /// Which column feeds each field; fields not listed stay unmapped.
    pub links: Vec<FieldLink>,
}

/// Why a template could not be read or written.
#[derive(Debug)]
pub enum TemplateError {
    /// The file could not be read or written.
    Io(std::io::Error),
    /// The text is not a template: the line (from 1) and what is wrong.
    Parse(usize, String),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::Io(e) => e.fmt(f),
            TemplateError::Parse(line, what) => write!(f, "line {line}: {what}"),
        }
    }
}

impl std::error::Error for TemplateError {}

impl From<std::io::Error> for TemplateError {
    fn from(e: std::io::Error) -> Self {
        TemplateError::Io(e)
    }
}

impl Template {
    /// A template with `options` and no links.
    #[must_use]
    pub fn new(name: &str, options: ImportOptions) -> Self {
        Self {
            name: name.to_string(),
            options,
            links: Vec::new(),
        }
    }

    /// A template of what `imported` used and `mapping` chose for `targets`.
    #[must_use]
    pub fn capture(
        name: &str,
        imported: &Imported,
        targets: &[TargetField],
        mapping: &Mapping,
    ) -> Self {
        let links = targets
            .iter()
            .zip(&mapping.sources)
            .enumerate()
            .filter_map(|(i, (target, source))| {
                let column = imported.columns.get((*source)?)?;
                Some(FieldLink {
                    field: target.name.clone(),
                    source: column.source,
                    title: column.title.clone(),
                    transforms: mapping.transforms.get(i).cloned().unwrap_or_default(),
                })
            })
            .collect();
        Self {
            name: name.to_string(),
            options: imported.options.clone(),
            links,
        }
    }

    /// Imports all of `bytes` with the template's options, without a dialog.
    #[must_use]
    pub fn import(&self, bytes: &[u8]) -> Imported {
        import(bytes, &self.options)
    }

    /// The mapping the template gives `imported`. A field whose link finds
    /// no column (the title is gone, the position is hidden or past the end)
    /// stays unmapped, which [`Mapping::missing`] reports if it is required.
    #[must_use]
    pub fn mapping(&self, targets: &[TargetField], imported: &Imported) -> Mapping {
        let mut mapping = Mapping::new(self.links_sources(targets, imported));
        mapping.transforms = targets
            .iter()
            .map(|target| {
                self.links
                    .iter()
                    .find(|l| l.field == target.name)
                    .map(|l| l.transforms.clone())
                    .unwrap_or_default()
            })
            .collect();
        mapping
    }

    fn links_sources(&self, targets: &[TargetField], imported: &Imported) -> Vec<Option<usize>> {
        targets
            .iter()
            .map(|target| {
                let link = self.links.iter().find(|l| l.field == target.name)?;
                let titled = imported.columns.iter().any(|c| c.title.is_some());
                match &link.title {
                    // The file has titles: trust them, so a column that was
                    // renamed or removed is not replaced by whatever sits at
                    // its old position.
                    Some(title) if titled => {
                        let wanted = normal(title);
                        imported
                            .columns
                            .iter()
                            .position(|c| c.title.as_deref().is_some_and(|t| normal(t) == wanted))
                    }
                    _ => imported
                        .columns
                        .iter()
                        .position(|c| c.source == link.source),
                }
            })
            .collect()
    }

    /// The template as text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let o = &self.options;
        let s = &o.separators;
        let mut out = String::new();
        let mut put = |key: &str, value: &str| {
            out.push_str(key);
            out.push_str(" = ");
            out.push_str(&escape(value));
            out.push('\n');
        };
        put("name", &self.name);
        put("charset", charset_key(o.charset));
        put("from_row", &o.from_row.to_string());
        put("first_row_titles", &o.first_row_titles.to_string());
        put("separator.tab", &s.tab.to_string());
        put("separator.comma", &s.comma.to_string());
        put("separator.semicolon", &s.semicolon.to_string());
        put("separator.space", &s.space.to_string());
        put("separator.other", &s.other);
        put("separator.merge", &s.merge.to_string());
        put("quote", &o.quote.map_or(String::new(), String::from));
        put("trim", &o.trim.to_string());
        put("quoted_as_text", &o.quoted_as_text.to_string());
        put(
            "detect_special_numbers",
            &o.detect_special_numbers.to_string(),
        );
        put("decimal", &o.decimal.to_string());
        put(
            "thousands",
            &o.thousands.map_or(String::new(), String::from),
        );
        put("date_order", order_key(o.date_order));
        for (i, t) in o.columns.iter().enumerate() {
            if *t != ColumnType::Standard {
                put(&format!("column.{}", i + 1), type_key(*t));
            }
        }
        for l in &self.links {
            out.push_str(&format!(
                "map = {}\t{}\t{}\n",
                l.source,
                escape(&l.field),
                escape(l.title.as_deref().unwrap_or_default())
            ));
            for t in &l.transforms {
                let mut parts = vec![escape(&l.field)];
                parts.extend(transform_parts(t));
                out.push_str(&format!("transform = {}\n", parts.join("\t")));
            }
        }
        out
    }

    /// Reads a template from text written by [`to_text`](Self::to_text).
    /// Keys left out keep the [`ImportOptions::default`] value (except the
    /// separators, which are off unless listed); unknown keys
    /// and blank or `#` lines are ignored, so a newer file still loads.
    ///
    /// # Errors
    ///
    /// [`TemplateError::Parse`] when a value is not valid for its key, or
    /// the template has no name.
    pub fn from_text(text: &str) -> Result<Self, TemplateError> {
        let mut t = Template::new("", ImportOptions::default());
        // Separators listed are the only ones on, not added to the default comma.
        t.options.separators = Separators::default();
        for (n, line) in text.lines().enumerate() {
            let n = n + 1;
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = |what: &str| TemplateError::Parse(n, what.to_string());
            let (key, value) = line
                .split_once(" = ")
                .ok_or_else(|| bad("expected `key = value`"))?;
            if key == "map" {
                let mut parts = value.split('\t');
                let source = parts.next().and_then(|p| p.parse().ok());
                let field = parts.next().map(unescape);
                let title = parts.next().map(unescape).filter(|t| !t.is_empty());
                let (Some(source), Some(field)) = (source, field) else {
                    return Err(bad("expected `map = position<TAB>field<TAB>title`"));
                };
                t.links.push(FieldLink {
                    field,
                    source,
                    title,
                    transforms: Vec::new(),
                });
                continue;
            }
            if key == "transform" {
                let mut parts = value.split('\t').map(unescape);
                let field = parts.next().unwrap_or_default();
                let transform = transform_from(parts.collect())
                    .ok_or_else(|| bad("expected `transform = field<TAB>kind<TAB>arguments`"))?;
                // Written after its `map` line, so the link exists by now.
                let link = t
                    .links
                    .iter_mut()
                    .rfind(|l| l.field == field)
                    .ok_or_else(|| bad("transform for a field with no `map` line before it"))?;
                link.transforms.push(transform);
                continue;
            }
            let value = unescape(value);
            let flag = || match value.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(bad("expected true or false")),
            };
            let single = || {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Ok(Some(c)),
                    (None, _) => Ok(None),
                    _ => Err(bad("expected one character")),
                }
            };
            let o = &mut t.options;
            match key {
                "name" => t.name = value.clone(),
                "charset" => {
                    o.charset = charset_from(&value).ok_or_else(|| bad("unknown charset"))?
                }
                "from_row" => {
                    o.from_row = value
                        .parse::<usize>()
                        .ok()
                        .filter(|&r| r >= 1)
                        .ok_or_else(|| bad("expected a row number from 1"))?;
                }
                "first_row_titles" => o.first_row_titles = flag()?,
                "separator.tab" => o.separators.tab = flag()?,
                "separator.comma" => o.separators.comma = flag()?,
                "separator.semicolon" => o.separators.semicolon = flag()?,
                "separator.space" => o.separators.space = flag()?,
                "separator.other" => o.separators.other = value.clone(),
                "separator.merge" => o.separators.merge = flag()?,
                "quote" => o.quote = single()?,
                "trim" => o.trim = flag()?,
                "quoted_as_text" => o.quoted_as_text = flag()?,
                "detect_special_numbers" => o.detect_special_numbers = flag()?,
                "decimal" => o.decimal = single()?.ok_or_else(|| bad("expected one character"))?,
                "thousands" => o.thousands = single()?,
                "date_order" => {
                    o.date_order = order_from(&value).ok_or_else(|| bad("unknown date order"))?;
                }
                _ => {
                    if let Some(index) = key.strip_prefix("column.") {
                        let index = index
                            .parse::<usize>()
                            .ok()
                            .filter(|&i| i >= 1)
                            .ok_or_else(|| bad("expected column.N with N from 1"))?;
                        let column_type =
                            type_from(&value).ok_or_else(|| bad("unknown column type"))?;
                        o.set_column_type(index - 1, column_type);
                    }
                }
            }
        }
        if t.name.is_empty() {
            return Err(TemplateError::Parse(1, "the template has no name".into()));
        }
        Ok(t)
    }

    /// Writes the template to `path`.
    ///
    /// # Errors
    ///
    /// [`TemplateError::Io`] when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), TemplateError> {
        Ok(std::fs::write(path, self.to_text())?)
    }

    /// Reads the template in `path`.
    ///
    /// # Errors
    ///
    /// [`TemplateError::Io`] when the file cannot be read,
    /// [`TemplateError::Parse`] when it is not a template.
    pub fn load(path: &Path) -> Result<Self, TemplateError> {
        Self::from_text(&std::fs::read_to_string(path)?)
    }

    /// Writes the template into `dir` (made if needed) as `<name>.tvt`, with
    /// characters a file name cannot hold replaced by `_`. A template of the
    /// same name is replaced. Returns the path written.
    ///
    /// # Errors
    ///
    /// [`TemplateError::Io`] when the directory or file cannot be written.
    pub fn save_in(&self, dir: &Path) -> Result<PathBuf, TemplateError> {
        std::fs::create_dir_all(dir)?;
        let file: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || " -.()".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(format!("{}.{EXTENSION}", file.trim_matches('.')));
        self.save(&path)?;
        Ok(path)
    }

    /// Every template in `dir`, by name. A missing directory has none, and a
    /// file that is not a template is skipped.
    #[must_use]
    pub fn load_all(dir: &Path) -> Vec<Self> {
        let mut found: Vec<Self> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == EXTENSION))
            .filter_map(|p| Self::load(&p).ok())
            .collect();
        found.sort_by_key(|t| t.name.to_lowercase());
        found
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The kind of a transform and its arguments, as they follow the field name.
fn transform_parts(t: &Transform) -> Vec<String> {
    match t {
        Transform::Trim => vec!["trim".into()],
        Transform::Upper => vec!["upper".into()],
        Transform::Lower => vec!["lower".into()],
        Transform::Replace { from, to } => vec!["replace".into(), escape(from), escape(to)],
        Transform::Map { pairs, default } => {
            let mut v = vec!["map".to_string()];
            for (from, to) in pairs {
                v.push(escape(from));
                v.push(escape(to));
            }
            // An odd argument at the end is the default.
            v.extend(default.iter().map(|d| escape(d)));
            v
        }
        Transform::Prefix(p) => vec!["prefix".into(), escape(p)],
        Transform::Suffix(p) => vec!["suffix".into(), escape(p)],
        Transform::Expr(e) => vec!["expr".into(), escape(e)],
    }
}

/// The inverse of [`transform_parts`], from unescaped parts.
fn transform_from(parts: Vec<String>) -> Option<Transform> {
    let (kind, args) = parts.split_first()?;
    Some(match (kind.as_str(), args) {
        ("trim", []) => Transform::Trim,
        ("upper", []) => Transform::Upper,
        ("lower", []) => Transform::Lower,
        ("replace", [from, to]) => Transform::Replace {
            from: from.clone(),
            to: to.clone(),
        },
        ("map", args) => Transform::Map {
            pairs: args
                .chunks(2)
                .filter(|p| p.len() == 2)
                .map(|p| (p[0].clone(), p[1].clone()))
                .collect(),
            default: (args.len() % 2 == 1).then(|| args[args.len() - 1].clone()),
        },
        ("prefix", [p]) => Transform::Prefix(p.clone()),
        ("suffix", [p]) => Transform::Suffix(p.clone()),
        ("expr", [e]) => Transform::Expr(e.clone()),
        _ => return None,
    })
}

fn charset_key(c: Charset) -> &'static str {
    match c {
        Charset::Utf8 => "utf-8",
        Charset::Utf16Le => "utf-16le",
        Charset::Utf16Be => "utf-16be",
        Charset::Windows1252 => "windows-1252",
        Charset::Latin1 => "iso-8859-1",
    }
}

fn charset_from(key: &str) -> Option<Charset> {
    Charset::ALL.into_iter().find(|&c| charset_key(c) == key)
}

fn order_key(o: DateOrder) -> &'static str {
    match o {
        DateOrder::Dmy => "dmy",
        DateOrder::Mdy => "mdy",
        DateOrder::Ymd => "ymd",
    }
}

fn order_from(key: &str) -> Option<DateOrder> {
    DateOrder::ALL.into_iter().find(|&o| order_key(o) == key)
}

fn type_key(t: ColumnType) -> &'static str {
    match t {
        ColumnType::Standard => "standard",
        ColumnType::Text => "text",
        ColumnType::Date(DateOrder::Dmy) => "date-dmy",
        ColumnType::Date(DateOrder::Mdy) => "date-mdy",
        ColumnType::Date(DateOrder::Ymd) => "date-ymd",
        ColumnType::Hide => "hide",
    }
}

fn type_from(key: &str) -> Option<ColumnType> {
    ColumnType::ALL.into_iter().find(|&t| type_key(t) == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::detect_options;

    const BANK: &[u8] = b"Fecha;Concepto;Importe;Saldo\n02/10/2026;COMPRA;-45,20;1.234,56\n";

    fn full() -> Template {
        let mut o = detect_options(BANK, &ImportOptions::default());
        o.decimal = ',';
        o.thousands = Some('.');
        o.first_row_titles = true;
        o.quote = None;
        o.separators.other = "|\t".into();
        o.set_column_type(0, ColumnType::Date(DateOrder::Dmy));
        o.set_column_type(3, ColumnType::Hide);
        let imported = import(BANK, &o);
        let targets = [TargetField::new("amount"), TargetField::new("booked_on")];
        let mut mapping = Mapping::new(vec![Some(2), Some(0)]);
        mapping.transforms = vec![
            vec![],
            vec![
                Transform::Trim,
                Transform::Map {
                    pairs: vec![("H".into(), "Hom\tbre".into())],
                    default: Some("?".into()),
                },
                Transform::Prefix("a\tb".into()),
                Transform::Suffix("-".into()),
                Transform::Expr("value + \"\\\"".into()),
            ],
        ];
        Template::capture("Bank\nexport", &imported, &targets, &mapping)
    }

    #[test]
    fn text_round_trips_every_setting() {
        let t = full();
        assert_eq!(Template::from_text(&t.to_text()).unwrap(), t);
    }

    #[test]
    fn capture_links_by_file_position_and_title() {
        let t = full();
        assert_eq!(
            t.links[0],
            FieldLink {
                field: "amount".into(),
                source: 2,
                title: Some("Importe".into()),
                transforms: vec![],
            }
        );
    }

    #[test]
    fn the_mapping_follows_a_title_that_moved_else_the_position() {
        let t = full();
        let targets = [TargetField::new("amount"), TargetField::new("booked_on")];
        let moved = b"Concepto;Importe;Fecha;Saldo\n3;2;02/10/2026;1\n";
        let imported = t.import(moved);
        let m = t.mapping(&targets, &imported);
        let titles: Vec<_> = m
            .sources
            .iter()
            .map(|s| imported.columns[s.unwrap()].title.clone().unwrap())
            .collect();
        assert_eq!(titles, ["Importe", "Fecha"]);

        // A title the file lacks is not replaced by the old position.
        let gone = t.import(b"Fecha;Concepto;Total;Saldo\n1;2;3;4\n");
        assert_eq!(t.mapping(&targets, &gone).sources[0], None);

        let mut untitled = t.clone();
        untitled.options.first_row_titles = false;
        untitled.links[0].title = None;
        let imported = untitled.import(b"a;b;c;d\n");
        assert_eq!(untitled.mapping(&targets, &imported).sources[0], Some(2));
    }

    #[test]
    fn a_link_to_a_hidden_column_leaves_the_field_unmapped() {
        let mut t = full();
        t.links[0].title = None;
        t.links[0].source = 3; // hidden by the template
        let targets = [TargetField::new("amount").required()];
        let m = t.mapping(&targets, &t.import(BANK));
        assert_eq!(m.sources, vec![None]);
        assert_eq!(m.missing(&targets).len(), 1);
    }

    #[test]
    fn unknown_keys_and_comments_are_ignored_and_bad_values_name_their_line() {
        let t = Template::from_text("# note\nname = x\nfuture = 1\n").unwrap();
        let expected = ImportOptions {
            separators: Separators::default(),
            ..ImportOptions::default()
        };
        assert_eq!(t.options, expected);
        let e = Template::from_text("name = x\nfrom_row = 0\n").unwrap_err();
        assert!(matches!(e, TemplateError::Parse(2, _)));
        assert!(Template::from_text("from_row = 2\n").is_err()); // no name
    }

    #[test]
    fn templates_save_into_a_directory_and_load_back_by_name() {
        let dir = std::env::temp_dir().join(format!("tvt-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut b = Template::new("b/second", ImportOptions::default());
        b.options.from_row = 3;
        let a = Template::new("A first", ImportOptions::default());
        b.save_in(&dir).unwrap();
        a.save_in(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "not a template").unwrap();
        assert_eq!(Template::load_all(&dir), vec![a, b]);
        assert!(Template::load_all(&dir.join("missing")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_mapping_carries_the_links_transforms() {
        let t = full();
        let targets = [TargetField::new("amount"), TargetField::new("booked_on")];
        let m = t.mapping(&targets, &t.import(BANK));
        assert_eq!(m.transforms[1].len(), 5);
        assert!(m.transforms[0].is_empty());
    }

    #[test]
    fn a_transform_without_a_map_line_is_an_error() {
        let e = Template::from_text("name = x\ntransform = a\ttrim\n").unwrap_err();
        assert!(matches!(e, TemplateError::Parse(2, _)));
        let e = Template::from_text("name = x\nmap = 0\ta\t\ntransform = a\tbogus\n").unwrap_err();
        assert!(matches!(e, TemplateError::Parse(3, _)));
    }
}
