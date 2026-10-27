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
    /// Evaluates an [`evalexpr`] expression where `value` is the field: text
    /// is a string, a number a float, a date its `YYYY-MM-DD` string.
    /// For example `if(value == "H", "Hombre", "Mujer")` or
    /// `str::to_uppercase(value)`. A string result is text, a number a
    /// number; an expression that fails leaves the value as it was.
    Expr(String),
}

impl Transform {
    /// `value` after the change. [`Value::Empty`] stays empty, except
    /// through [`Expr`](Self::Expr), which sees it as an empty string.
    #[must_use]
    pub fn apply(&self, value: &Value) -> Value {
        match self {
            Transform::Expr(expr) => eval(expr, value).unwrap_or_else(|| value.clone()),
            _ if *value == Value::Empty => Value::Empty,
            Transform::Trim => text_of(value, |t| t.trim().to_string()),
            Transform::Upper => text_of(value, str::to_uppercase),
            Transform::Lower => text_of(value, str::to_lowercase),
            Transform::Replace { from, to } if !from.is_empty() => {
                text_of(value, |t| t.replace(from.as_str(), to))
            }
            Transform::Replace { .. } => value.clone(),
            Transform::Map { pairs, default } => {
                let text = value.to_string();
                match pairs.iter().find(|(k, _)| *k == text) {
                    Some((_, v)) => Value::Text(v.clone()),
                    None => default.clone().map_or_else(|| value.clone(), Value::Text),
                }
            }
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
        evalexpr::eval_with_context(expr, &context(&Value::Text("x".into())))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Applies `transforms` in order.
#[must_use]
pub fn apply_all(transforms: &[Transform], value: Value) -> Value {
    transforms.iter().fold(value, |v, t| t.apply(&v))
}

/// Text changed by `f`; a number or date is changed through its text form.
fn text_of(value: &Value, f: impl Fn(&str) -> String) -> Value {
    match value {
        Value::Text(t) => Value::Text(f(t)),
        other => {
            let changed = f(&other.to_string());
            if changed == other.to_string() {
                other.clone()
            } else {
                Value::Text(changed)
            }
        }
    }
}

fn context(value: &Value) -> HashMapContext<DefaultNumericTypes> {
    let v = match value {
        Value::Empty => evalexpr::Value::String(String::new()),
        Value::Number(n) => evalexpr::Value::Float(*n),
        other => evalexpr::Value::String(other.to_string()),
    };
    let mut ctx = HashMapContext::new();
    // A fresh context cannot refuse a variable.
    let _ = ctx.set_value("value".to_string(), v);
    ctx
}

fn eval(expr: &str, value: &Value) -> Option<Value> {
    Some(
        match evalexpr::eval_with_context(expr, &context(value)).ok()? {
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
        assert_eq!(r.apply(&Value::Number(5.0)), Value::Number(5.0));
    }

    #[test]
    fn expressions_see_the_field_as_value() {
        let e = Transform::Expr(r#"if(value == "H", "Hombre", "Mujer")"#.into());
        assert_eq!(e.apply(&text("H")), text("Hombre"));
        assert_eq!(e.apply(&text("F")), text("Mujer"));
        let n = Transform::Expr("value * 100".into());
        assert_eq!(n.apply(&Value::Number(1.5)), Value::Number(150.0));
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
