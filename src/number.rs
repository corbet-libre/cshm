use std::{fmt, str::FromStr};

use bigdecimal::BigDecimal;
use serde_json::Value;

use crate::{FieldType, ValidationReason};

/// A malformed or computationally excessive number, without its input value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NumberError;

impl fmt::Display for NumberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("number exceeds the supported syntax or precision budget")
    }
}

impl std::error::Error for NumberError {}

/// Parse exact JSON numeric syntax with bounded work on native and wasm.
/// At most 128 bytes, an explicit exponent in -128..=128 and resulting decimal
/// scale in -128..=128 are supported. Bounds are checked before bignum parsing.
pub fn parse_number(text: &str) -> Result<BigDecimal, NumberError> {
    if text.len() > 128 {
        return Err(NumberError);
    }
    if let Some((_, exponent)) = text.split_once(['e', 'E']) {
        let exponent: i32 = exponent.parse().map_err(|_| NumberError)?;
        if !(-128..=128).contains(&exponent) {
            return Err(NumberError);
        }
    }
    serde_json::from_str::<serde_json::Number>(text).map_err(|_| NumberError)?;
    let number = BigDecimal::from_str(text).map_err(|_| NumberError)?;
    if !(-128..=128).contains(&number.as_bigint_and_exponent().1) {
        return Err(NumberError);
    }
    Ok(number)
}

fn value_number(value: &Value) -> Result<BigDecimal, ValidationReason> {
    let number = value.as_number().ok_or(ValidationReason::WrongType)?;
    parse_number(number.as_str()).map_err(|_| ValidationReason::NumberLimit)
}

pub(crate) fn validate(kind: &FieldType, value: &Value) -> Option<Vec<ValidationReason>> {
    if *kind == FieldType::Location {
        let valid = value.as_object().is_some_and(|coordinates| {
            coordinates.len() == 2
                && [("latitude", 90), ("longitude", 180)]
                    .into_iter()
                    .all(|(key, bound)| {
                        coordinates
                            .get(key)
                            .and_then(|v| value_number(v).ok())
                            .is_some_and(|n| n >= -bound && n <= bound)
                    })
        });
        return Some(if valid {
            Vec::new()
        } else {
            vec![ValidationReason::InvalidLocation]
        });
    }
    let (min, max) = kind.number_bounds()?;
    let number = match value_number(value) {
        Ok(number) => number,
        Err(reason) => return Some(vec![reason]),
    };
    let mut reasons = Vec::new();
    if matches!(kind, FieldType::Integer { .. }) && !number.is_integer() {
        reasons.push(ValidationReason::WrongType);
    }
    // Schema bounds are validated Decimal values: their display is bounded JSON.
    if min.is_some_and(|min| number < parse_number(&min.to_string()).expect("decimal bound")) {
        reasons.push(ValidationReason::BelowMinimum);
    }
    if max.is_some_and(|max| number > parse_number(&max.to_string()).expect("decimal bound")) {
        reasons.push(ValidationReason::AboveMaximum);
    }
    Some(reasons)
}
