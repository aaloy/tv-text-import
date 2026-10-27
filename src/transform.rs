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
    /// Evaluates an [`evalexpr`] formula where `value` is the field's number,
    /// such as `value * -1`, `value / 100` or `if(value < 0, 0, value)`.
    /// For numbers only; a formula that fails or does not give a number
    /// leaves the value as it was.
    Expr(String),
}

impl Transform {
    /// `value` after the change. Each transform has a kind of value it
    /// works on: [`Expr`](Self::Expr) on numbers, all the others on text.
    /// Any other value, a date or an empty field comes back as it was, so
    /// the same transforms can sit on a field whatever the file holds.
    #[must_use]
    pub fn apply(&self, value: &Value) -> Value {
        if let Transform::Expr(expr) = self {
            return match value {
                Value::Number(n) => eval(expr, *n).unwrap_or_else(|| value.clone()),
                _ => value.clone(),
            };
        }
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
            Transform::Expr(_) => unreachable!("handled above"),
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
        match evalexpr::eval_with_context(expr, &context(1.0)) {
            Ok(evalexpr::Value::Float(_) | evalexpr::Value::Int(_)) => Ok(()),
            Ok(_) => Err("the formula must give a number".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Applies `transforms` in order.
#[must_use]
pub fn apply_all(transforms: &[Transform], value: Value) -> Value {
    transforms.iter().fold(value, |v, t| t.apply(&v))
}

fn context(number: f64) -> HashMapContext<DefaultNumericTypes> {
    let mut ctx = HashMapContext::new();
    // A fresh context cannot refuse a variable.
    let _ = ctx.set_value("value".to_string(), evalexpr::Value::Float(number));
    ctx
}

fn eval(expr: &str, number: f64) -> Option<Value> {
    match evalexpr::eval_with_context(expr, &context(number)).ok()? {
        evalexpr::Value::Float(f) => Some(Value::Number(f)),
        #[allow(clippy::cast_precision_loss)]
        evalexpr::Value::Int(i) => Some(Value::Number(i as f64)),
        _ => None,
    }
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
    fn text_transforms_leave_everything_but_text_alone() {
        let date = Value::Date(chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        for t in [Transform::Upper, Transform::Prefix("x".into()), sex()] {
            assert_eq!(t.apply(&Value::Number(5.0)), Value::Number(5.0));
            assert_eq!(t.apply(&date), date);
            assert_eq!(t.apply(&Value::Empty), Value::Empty);
        }
    }

    #[test]
    fn formulas_change_numbers_only() {
        let date = Value::Date(chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        let neg = Transform::Expr("value * -1".into());
        assert_eq!(neg.apply(&Value::Number(45.2)), Value::Number(-45.2));
        let cap = Transform::Expr("if(value < 0, 0, value)".into());
        assert_eq!(cap.apply(&Value::Number(-3.0)), Value::Number(0.0));
        assert_eq!(cap.apply(&Value::Number(3.0)), Value::Number(3.0));
        assert_eq!(neg.apply(&text("12")), text("12"));
        assert_eq!(neg.apply(&date), date);
        assert_eq!(neg.apply(&Value::Empty), Value::Empty);
    }

    #[test]
    fn a_failing_formula_leaves_the_value_and_check_says_why() {
        let bad = Transform::Expr("value +".into());
        assert_eq!(bad.apply(&Value::Number(1.0)), Value::Number(1.0));
        assert!(bad.check().is_err());
        assert!(Transform::Expr("\"text\"".into()).check().is_err());
        assert!(Transform::Expr("value / 100".into()).check().is_ok());
        assert!(Transform::Upper.check().is_ok());
    }

    #[test]
    fn transforms_apply_in_order() {
        let v = apply_all(&[Transform::Trim, Transform::Upper, sex()], text(" h "));
        assert_eq!(v, text("Hombre"));
    }
}
