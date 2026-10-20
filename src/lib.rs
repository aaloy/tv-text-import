//! A Text Import dialog for [turbo-vision], after LibreOffice Calc's.
//!
//! It reads a delimited text file (CSV, TSV, semicolon-separated bank
//! exports and the like) the way the user says: character set, first row,
//! separators, string delimiter, trimming, number and date formats, and a
//! type for each column, with a live preview. On OK it hands back typed rows.
//!
//! ```no_run
//! use turbo_vision::app::Application;
//! use tv_text_import::{ImportOptions, TextImportDialog};
//!
//! # fn main() -> turbo_vision::core::error::Result<()> {
//! let mut app = Application::new()?;
//! let bytes = std::fs::read("movements.csv")?;
//! if let Some(imported) = TextImportDialog::new("movements.csv", bytes).execute(&mut app) {
//!     for row in &imported.rows {
//!         // row[i] is a Value: Empty, Text, Number or Date.
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Without the dialog, [`import()`] does the same work from options set in
//! code, and [`detect_options`] guesses the character set and separator.
//!
//! [turbo-vision]: https://github.com/aovestdipaperino/turbo-vision-4-rust

pub mod charset;
pub mod dialog;
pub mod import;
pub mod options;
pub mod split;
pub mod value;

pub use charset::Charset;
pub use dialog::TextImportDialog;
pub use import::{Imported, ImportedColumn, detect_options, import};
pub use options::{ColumnType, DateOrder, ImportOptions, Separators};
pub use value::Value;
