//! Small changes to a mapped field's value: trim, change case, replace text,
//! look a code up in a table (`H` to `Hombre`), or compute it with an
//! expression.

use evalexpr::{ContextWithMutableVariables, DefaultNumericTypes, HashMapContext};

use crate::value::Value;

/// One change to a value. A field applies its transforms in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transform {
    /// Removes spaces around text.
    Trim,
    /// Text in capitals.
    Upper,
    /// Text in lowercase.
    Lower,
    /// Replaces every `from` in the text with `to`.
    Replace {
        /// What to look for.
        from: String,
        /// What to put instead.
        to: String,
    },
    /// Looks the value's text up in `pairs` (exact match). A value not
    /// listed becomes `default`, or stays as it is when that is `None`.
    Map {
        /// Pairs of what the file has and what to import instead.
        pairs: Vec<(String, String)>,
        /// What an unlisted value becomes.
        default: Option<String>,
    },
    /// Puts text before the value.
    Prefix(String),
    /// Puts text after the value.
    Suffix(String),
    /// Evaluates an [`evalexpr`] expression where `value` is the field's text.
    /// For example `if(value == "H", "Hombre", "Mujer")` or
    /// `str::to_uppercase(value)`. A string result is text, a number a
    /// number; an expression that fails leaves the value as it was.
    Expr(String),
}

impl Transform {
    /// `value` after the change. Only text is changed: a number, a date or
    /// an empty field comes back as it was, so the same transforms can sit
    /// on a field whatever the file holds.
    #[must_use]
    pub fn apply(&self, value: &Value) -> Value {
        let Value::Text(text) = value else {
            return value.clone();
        };
        match self {
            Transform::Trim => Value::Text(text.trim().to_string()),
            Transform::Upper => Value::Text(text.to_uppercase()),
            Transform::Lower => Value::Text(text.to_lowercase()),
            Transform::Replace { from, to } if !from.is_empty() => {
                Value::Text(text.replace(from.as_str(), to))
            }
            Transform::Replace { .. } => value.clone(),
            Transform::Prefix(p) => Value::Text(format!("{p}{text}")),
            Transform::Suffix(p) => Value::Text(format!("{text}{p}")),
            Transform::Map { pairs, default } => match pairs.iter().find(|(k, _)| k == text) {
                Some((_, v)) => Value::Text(v.clone()),
                None => default.clone().map_or_else(|| value.clone(), Value::Text),
            },
            Transform::Expr(expr) => eval(expr, text).unwrap_or_else(|| value.clone()),
        }
    }

    /// Whether the transform can run: only an [`Expr`](Self::Expr) can be
    /// wrong, and this reports why. The dialog uses it before accepting one.
    ///
    /// # Errors
    ///
    /// The message for an expression that does not parse or evaluate.
    pub fn check(&self) -> Result<(), String> {
        let Transform::Expr(expr) = self else {
            return Ok(());
        };
        evalexpr::eval_with_context(expr, &context("x"))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Applies `transforms` in order.
#[must_use]
pub fn apply_all(transforms: &[Transform], value: Value) -> Value {
    transforms.iter().fold(value, |v, t| t.apply(&v))
}

fn context(text: &str) -> HashMapContext<DefaultNumericTypes> {
    let v = evalexpr::Value::String(text.to_string());
    let mut ctx = HashMapContext::new();
    // A fresh context cannot refuse a variable.
    let _ = ctx.set_value("value".to_string(), v);
    ctx
}

fn eval(expr: &str, text: &str) -> Option<Value> {
    Some(
        match evalexpr::eval_with_context(expr, &context(text)).ok()? {
            evalexpr::Value::String(s) if s.is_empty() => Value::Empty,
            evalexpr::Value::String(s) => Value::Text(s),
            evalexpr::Value::Float(f) => Value::Number(f),
            #[allow(clippy::cast_precision_loss)]
            evalexpr::Value::Int(i) => Value::Number(i as f64),
            evalexpr::Value::Boolean(b) => Value::Text(b.to_string()),
            evalexpr::Value::Empty => Value::Empty,
            evalexpr::Value::Tuple(_) => return None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }

    fn sex() -> Transform {
        Transform::Map {
            pairs: vec![("H".into(), "Hombre".into()), ("F".into(), "Mujer".into())],
            default: None,
        }
    }

    #[test]
    fn map_translates_listed_codes_and_keeps_or_defaults_the_rest() {
        assert_eq!(sex().apply(&text("H")), text("Hombre"));
        assert_eq!(sex().apply(&text("X")), text("X"));
        let Transform::Map { pairs, .. } = sex() else {
            unreachable!()
        };
        let t = Transform::Map {
            pairs,
            default: Some("?".into()),
        };
        assert_eq!(t.apply(&text("X")), text("?"));
        assert_eq!(sex().apply(&Value::Empty), Value::Empty);
    }

    #[test]
    fn text_transforms() {
        assert_eq!(Transform::Trim.apply(&text("  a ")), text("a"));
        assert_eq!(Transform::Upper.apply(&text("ab")), text("AB"));
        assert_eq!(Transform::Lower.apply(&text("AB")), text("ab"));
        let r = Transform::Replace {
            from: "-".into(),
            to: "".into(),
        };
        assert_eq!(r.apply(&text("12-34")), text("1234"));
        assert_eq!(
            Transform::Prefix("ES".into()).apply(&text("12")),
            text("ES12")
        );
        assert_eq!(
            Transform::Suffix("!".into()).apply(&text("12")),
            text("12!")
        );
    }

    #[test]
    fn only_text_is_changed() {
        let date = Value::Date(chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        for t in [
            Transform::Upper,
            Transform::Prefix("x".into()),
            sex(),
            Transform::Expr("\"x\"".into()),
        ] {
            assert_eq!(t.apply(&Value::Number(5.0)), Value::Number(5.0));
            assert_eq!(t.apply(&date), date);
            assert_eq!(t.apply(&Value::Empty), Value::Empty);
        }
    }

    #[test]
    fn expressions_see_the_field_as_value() {
        let e = Transform::Expr(r#"if(value == "H", "Hombre", "Mujer")"#.into());
        assert_eq!(e.apply(&text("H")), text("Hombre"));
        assert_eq!(e.apply(&text("F")), text("Mujer"));
        let n = Transform::Expr("str::from(1 + 1)".into());
        assert_eq!(n.apply(&text("a")), text("2"));
        let u = Transform::Expr("str::to_uppercase(value)".into());
        assert_eq!(u.apply(&text("ab")), text("AB"));
    }

    #[test]
    fn a_failing_expression_leaves_the_value_and_check_says_why() {
        let bad = Transform::Expr("value +".into());
        assert_eq!(bad.apply(&text("a")), text("a"));
        assert!(bad.check().is_err());
        assert!(Transform::Expr("value".into()).check().is_ok());
    }

    #[test]
    fn transforms_apply_in_order() {
        let v = apply_all(&[Transform::Trim, Transform::Upper, sex()], text(" h "));
        assert_eq!(v, text("Hombre"));
    }
}
