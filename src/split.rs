//! Splitting decoded text into rows of fields.

use crate::options::{ImportOptions, Separators};

/// One field as the text had it, before typing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Field {
    /// The field's text: quotes removed, doubled quotes undone, trimmed when
    /// trimming is on.
    pub text: String,
    /// Whether the field was quoted.
    pub quoted: bool,
}

impl Field {
    /// An unquoted field.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            quoted: false,
        }
    }
}

/// Splits `text` into rows of fields as `options` say, starting at
/// [`ImportOptions::from_row`] and stopping after `limit` rows if given.
/// When the first row holds titles, it is always kept and `from_row` counts
/// the rows after it.
///
/// Rows end at `\n`, `\r\n` or a lone `\r`, except inside a quoted field.
/// A quote opens a field only at its start (after spaces, when trimming);
/// inside it, two quotes stand for one, and one closes it. Text after the
/// closing quote, up to the next separator, is kept. A quote left open at the
/// end of the text keeps what it collected. An empty line is a row with one
/// empty field; a line break at the very end makes no extra row.
#[must_use]
pub fn split(text: &str, options: &ImportOptions, limit: Option<usize>) -> Vec<Vec<Field>> {
    let mut splitter = Splitter::new(options);
    let skip = options.from_row.saturating_sub(1);
    let titles = usize::from(options.first_row_titles);
    // Rows `titles + 1 ..= titles + skip` are skipped; the title row is not.
    let kept = |seen: usize| seen <= titles || seen > titles + skip;
    let mut rows = Vec::new();
    let mut seen = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(row) = splitter.push(c) {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            seen += 1;
            if kept(seen) {
                rows.push(row);
                if limit.is_some_and(|limit| rows.len() >= limit) {
                    return rows;
                }
            }
        }
    }
    if let Some(row) = splitter.finish() {
        seen += 1;
        if kept(seen) {
            rows.push(row);
        }
    }
    rows
}

/// The splitting state machine, fed one character at a time.
struct Splitter<'a> {
    separators: &'a Separators,
    quote: Option<char>,
    trim: bool,
    row: Vec<Field>,
    field: String,
    /// The field opened with a quote.
    quoted: bool,
    /// Inside the quotes of a quoted field.
    in_quotes: bool,
    /// How much of `field` was inside the quotes; what follows came after the
    /// closing quote.
    quoted_len: usize,
    /// A quote was just read inside quotes: either a doubled quote or the end.
    quote_pending: bool,
    /// The last thing read ended a field, so an empty field here comes from
    /// two separators in a row.
    after_separator: bool,
    /// Something was read since the last row ended.
    started: bool,
}

impl<'a> Splitter<'a> {
    fn new(options: &'a ImportOptions) -> Self {
        Self {
            separators: &options.separators,
            quote: options.quote,
            trim: options.trim,
            row: Vec::new(),
            field: String::new(),
            quoted: false,
            in_quotes: false,
            quoted_len: 0,
            quote_pending: false,
            after_separator: false,
            started: false,
        }
    }

    /// Reads `c`; returns a row when `c` ended one.
    fn push(&mut self, c: char) -> Option<Vec<Field>> {
        self.started = true;
        if self.in_quotes {
            if self.quote_pending {
                self.quote_pending = false;
                if Some(c) == self.quote {
                    self.field.push(c);
                    return None;
                }
                self.in_quotes = false; // the pending quote closed the field
                self.quoted_len = self.field.len();
            } else if Some(c) == self.quote {
                self.quote_pending = true;
                return None;
            } else {
                self.field.push(c);
                return None;
            }
        }
        if c == '\n' || c == '\r' {
            return Some(self.end_row());
        }
        if self.separators.contains(c) {
            self.end_field(true);
            return None;
        }
        if Some(c) == self.quote && !self.quoted && self.opens_quote() {
            self.field.clear();
            self.quoted = true;
            self.in_quotes = true;
            return None;
        }
        self.field.push(c);
        None
    }

    /// A quote opens the field when nothing but (trimmed) spaces came before.
    fn opens_quote(&self) -> bool {
        self.field.is_empty() || (self.trim && self.field.chars().all(char::is_whitespace))
    }

    fn end_field(&mut self, by_separator: bool) {
        if self.in_quotes {
            // Still open (the text ended inside quotes): all of it was quoted.
            self.quoted_len = self.field.len();
        }
        let text = std::mem::take(&mut self.field);
        let empty_run = by_separator
            && self.separators.merge
            && self.after_separator
            && text.is_empty()
            && !self.quoted;
        if !empty_run {
            let text = match (self.trim, self.quoted) {
                (false, _) => text,
                (true, false) => text.trim().to_string(),
                // Only the text after the closing quote is trimmed.
                (true, true) => {
                    let (inside, after) = text.split_at(self.quoted_len);
                    format!("{inside}{}", after.trim_end())
                }
            };
            self.row.push(Field {
                text,
                quoted: self.quoted,
            });
        }
        self.quoted = false;
        self.in_quotes = false;
        self.quoted_len = 0;
        self.quote_pending = false;
        self.after_separator = by_separator;
    }

    fn end_row(&mut self) -> Vec<Field> {
        self.end_field(false);
        self.after_separator = false;
        self.started = false;
        std::mem::take(&mut self.row)
    }

    /// The last row, if the text did not end with a line break.
    fn finish(&mut self) -> Option<Vec<Field>> {
        if !self.started {
            return None;
        }
        Some(self.end_row())
    }
}

/// The separator `text` most likely uses: the one of tab, `;`, `,` and `|`
/// that appears on every one of the first lines, the same number of times on
/// most of them, and most often. Quoted text is not counted. `None` when none
/// of them appears on every line (a single column, or fixed width).
#[must_use]
pub fn guess_separator(text: &str, quote: Option<char>) -> Option<char> {
    const CANDIDATES: [char; 4] = ['\t', ';', ',', '|'];
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    if lines.is_empty() {
        return None;
    }
    let mut best: Option<(char, usize, usize)> = None; // (separator, lines on the mode, mode)
    for candidate in CANDIDATES {
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| count_outside_quotes(l, candidate, quote))
            .collect();
        if counts.contains(&0) {
            continue;
        }
        let mode = most_common(&counts);
        let on_mode = counts.iter().filter(|&&n| n == mode).count();
        let better = best
            .is_none_or(|(_, best_on_mode, best_mode)| (on_mode, mode) > (best_on_mode, best_mode));
        if better {
            best = Some((candidate, on_mode, mode));
        }
    }
    best.map(|(c, _, _)| c)
}

fn count_outside_quotes(line: &str, c: char, quote: Option<char>) -> usize {
    let mut inside = false;
    let mut count = 0;
    for ch in line.chars() {
        if Some(ch) == quote {
            inside = !inside;
        } else if ch == c && !inside {
            count += 1;
        }
    }
    count
}

fn most_common(counts: &[usize]) -> usize {
    let mut best = (0, 0); // (occurrences, value)
    for &value in counts {
        let occurrences = counts.iter().filter(|&&n| n == value).count();
        if (occurrences, value) > best {
            best = (occurrences, value);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(separator: char) -> ImportOptions {
        ImportOptions {
            separators: Separators::single(separator),
            ..ImportOptions::default()
        }
    }

    fn texts(rows: &[Vec<Field>]) -> Vec<Vec<&str>> {
        rows.iter()
            .map(|r| r.iter().map(|f| f.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn plain_rows_with_any_line_ending() {
        let rows = split("a;b\r\nc;d\re;f\ng;h\n", &opts(';'), None);
        assert_eq!(
            texts(&rows),
            vec![
                vec!["a", "b"],
                vec!["c", "d"],
                vec!["e", "f"],
                vec!["g", "h"]
            ]
        );
    }

    #[test]
    fn quoted_fields_keep_separators_line_breaks_and_doubled_quotes() {
        let rows = split(
            "\"a;1\";\"say \"\"hi\"\"\";\"two\nlines\"\nx;y;z",
            &opts(';'),
            None,
        );
        assert_eq!(
            texts(&rows),
            vec![vec!["a;1", "say \"hi\"", "two\nlines"], vec!["x", "y", "z"]]
        );
        assert!(rows[0][0].quoted && !rows[1][0].quoted);
    }

    #[test]
    fn a_quote_inside_a_field_is_just_a_character() {
        let rows = split("5\" disk;it's", &opts(';'), None);
        assert_eq!(texts(&rows), vec![vec!["5\" disk", "it's"]]);
    }

    #[test]
    fn no_quote_character_reads_quotes_as_text() {
        let o = ImportOptions {
            quote: None,
            ..opts(',')
        };
        assert_eq!(texts(&split("\"a,b\"", &o, None)), vec![vec!["\"a", "b\""]]);
    }

    #[test]
    fn single_quote_as_the_string_delimiter() {
        let o = ImportOptions {
            quote: Some('\''),
            ..opts(',')
        };
        assert_eq!(texts(&split("'a,b',c", &o, None)), vec![vec!["a,b", "c"]]);
    }

    #[test]
    fn trimming_removes_spaces_around_unquoted_fields_and_outside_quotes() {
        let rows = split("  a  ;  \" b \"  ;c", &opts(';'), None);
        assert_eq!(texts(&rows), vec![vec!["a", " b ", "c"]]);
        let keep = ImportOptions {
            trim: false,
            ..opts(';')
        };
        assert_eq!(
            texts(&split("  a  ;c", &keep, None)),
            vec![vec!["  a  ", "c"]]
        );
    }

    #[test]
    fn several_separators_at_once() {
        let o = ImportOptions {
            separators: Separators {
                tab: true,
                semicolon: true,
                other: "|".into(),
                ..Separators::default()
            },
            ..ImportOptions::default()
        };
        assert_eq!(
            texts(&split("a\tb;c|d", &o, None)),
            vec![vec!["a", "b", "c", "d"]]
        );
    }

    #[test]
    fn merge_makes_runs_of_separators_one() {
        let mut o = opts(' ');
        o.trim = false;
        assert_eq!(
            texts(&split("a   b c", &o, None)),
            vec![vec!["a", "", "", "b", "c"]]
        );
        o.separators.merge = true;
        assert_eq!(
            texts(&split("a   b c", &o, None)),
            vec![vec!["a", "b", "c"]]
        );
    }

    #[test]
    fn empty_fields_and_empty_lines_are_kept() {
        let rows = split("a;;c\n\nd;", &opts(';'), None);
        assert_eq!(
            texts(&rows),
            vec![vec!["a", "", "c"], vec![""], vec!["d", ""]]
        );
    }

    #[test]
    fn from_row_skips_rows_and_limit_stops_early() {
        let o = ImportOptions {
            from_row: 3,
            ..opts(',')
        };
        assert_eq!(
            texts(&split("1\n2\n3\n4\n5", &o, Some(2))),
            vec![vec!["3"], vec!["4"]]
        );
        let past = ImportOptions {
            from_row: 9,
            ..opts(',')
        };
        assert!(split("1\n2", &past, None).is_empty());
    }

    #[test]
    fn skipped_rows_count_after_the_titles() {
        let o = ImportOptions {
            from_row: 3,
            first_row_titles: true,
            ..opts(',')
        };
        assert_eq!(
            texts(&split("t\n1\n2\n3\n4", &o, None)),
            vec![vec!["t"], vec!["3"], vec!["4"]]
        );
    }

    #[test]
    fn a_quote_left_open_keeps_what_it_collected() {
        assert_eq!(
            texts(&split("a,\"b\nc", &opts(','), None)),
            vec![vec!["a", "b\nc"]]
        );
    }

    #[test]
    fn a_skipped_row_with_a_quoted_line_break_counts_once() {
        let o = ImportOptions {
            from_row: 2,
            ..opts(',')
        };
        assert_eq!(
            texts(&split("\"x\ny\",1\nz,2", &o, None)),
            vec![vec!["z", "2"]]
        );
    }

    #[test]
    fn guess_picks_the_consistent_separator() {
        assert_eq!(
            guess_separator(
                "Fecha;Concepto;Importe\n01/10/2026;Compra, tienda;-45,20\n",
                Some('"')
            ),
            Some(';')
        );
        assert_eq!(guess_separator("a\tb\tc\n1\t2\t3\n", Some('"')), Some('\t'));
        assert_eq!(
            guess_separator("name,\"a; b\"\nx,y\n", Some('"')),
            Some(',')
        );
        assert_eq!(guess_separator("one column\nonly\n", Some('"')), None);
    }
}
