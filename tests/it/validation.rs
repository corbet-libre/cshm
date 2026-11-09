//! Real profile validation and JSON round trips.

use std::cell::Cell;

use crate::common::*;
use cshm::{ContactCheck, FieldType, Part, Profile, PublicProfile, Schema, ValidationReason};
use serde_json::{Value, json};

fn reasons(kind: FieldType, value: Value) -> Vec<ValidationReason> {
    schema_with(kind)
        .compile()
        .unwrap()
        .validate_profile(&profile_with(value))
        .violations
        .into_iter()
        .map(|issue| issue.reason)
        .collect()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn all_types_survive_real_json_round_trips_and_validate() {
    let schema = all_types();
    let encoded = serde_json::to_string(&schema).unwrap();
    assert!(encoded.contains("\"100.0000000000000000001\""));
    let decoded: Schema = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, schema);
    let profile = complete_profile();
    let decoded_profile: Profile =
        serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
    assert_eq!(decoded_profile, profile);
    let compiled = decoded.compile().unwrap();
    assert_eq!(compiled.schema(), &schema);
    assert!(compiled.validate_profile(&decoded_profile).is_valid());
    let public = PublicProfile::from(&profile);
    let public_wire = serde_json::to_string(&public).unwrap();
    assert!(!public_wire.contains("private"));
    assert!(!public_wire.contains("Long walks"));
    assert!(
        compiled
            .validate_public(&serde_json::from_str(&public_wire).unwrap())
            .is_valid()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn integer_bounds_are_inclusive_and_do_not_round_large_integers() {
    for value in [json!(18), json!(120), json!(34), json!(18.0)] {
        assert!(reasons(integer(18, 120), value).is_empty());
    }
    for (value, reason) in [
        (json!(17), ValidationReason::BelowMinimum),
        (json!(121), ValidationReason::AboveMaximum),
        (json!(18.5), ValidationReason::WrongType),
        (json!("34"), ValidationReason::WrongType),
    ] {
        assert_eq!(reasons(integer(18, 120), value), vec![reason]);
    }
    let limit = 9_007_199_254_740_993_i64;
    assert!(reasons(integer(limit, limit), json!(limit)).is_empty());
    assert_eq!(
        reasons(integer(limit, limit), json!(limit - 1)),
        vec![ValidationReason::BelowMinimum]
    );
    assert_eq!(
        reasons(integer(limit, limit), json!(limit + 1)),
        vec![ValidationReason::AboveMaximum]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn integer_default_bounds_cover_exactly_signed_64_bit_values() {
    let kind = FieldType::Integer {
        min: None,
        max: None,
    };
    for value in [json!(i64::MIN), json!(i64::MAX), json!(0)] {
        assert!(reasons(kind.clone(), value).is_empty());
    }
    for (text, reason) in [
        ("9223372036854775808", ValidationReason::AboveMaximum),
        ("-9223372036854775809", ValidationReason::BelowMinimum),
    ] {
        assert_eq!(
            reasons(kind.clone(), serde_json::from_str(text).unwrap()),
            vec![reason]
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn decimal_validation_preserves_sub_float_precision() {
    let kind = decimal(Some("0.1000000000000000001"), Some("0.1000000000000000002"));
    for number in [
        "0.1000000000000000001",
        "0.10000000000000000015",
        "0.1000000000000000002",
    ] {
        assert!(reasons(kind.clone(), serde_json::from_str(number).unwrap()).is_empty());
    }
    for (number, reason) in [
        ("0.10000000000000000009", ValidationReason::BelowMinimum),
        ("0.10000000000000000021", ValidationReason::AboveMaximum),
    ] {
        assert_eq!(
            reasons(kind.clone(), serde_json::from_str(number).unwrap()),
            vec![reason]
        );
    }
    assert!(reasons(decimal(None, None), serde_json::from_str("1e100").unwrap()).is_empty());
    assert!(reasons(decimal(Some("-1"), None), json!(-1)).is_empty());
    assert_eq!(
        reasons(decimal(None, Some("1")), json!(2)),
        vec![ValidationReason::AboveMaximum]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn decimal_profile_round_trip_keeps_its_digits() {
    let value: Value = serde_json::from_str("0.1000000000000000001").unwrap();
    let profile = profile_with(value.clone());
    let wire = serde_json::to_string(&profile).unwrap();
    let decoded: Profile = serde_json::from_str(&wire).unwrap();
    assert_eq!(decoded.public["answer"], value);
    assert!(
        schema_with(decimal(
            Some("0.1000000000000000001"),
            Some("0.1000000000000000001")
        ))
        .compile()
        .unwrap()
        .validate_profile(&decoded)
        .is_valid()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn choice_uses_option_ids_not_labels_and_does_not_coerce() {
    let mut kind = choice(&["forest", "mountain"]);
    if let FieldType::Choice { options } = &mut kind {
        options[0].label = "Woodland".into();
    }
    assert!(reasons(kind.clone(), json!("forest")).is_empty());
    for value in [
        json!("Woodland"),
        json!("FOREST"),
        json!(["forest"]),
        json!(false),
    ] {
        assert!(!reasons(kind.clone(), value).is_empty());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn boolean_accepts_only_booleans() {
    for value in [json!(true), json!(false)] {
        assert!(reasons(FieldType::YesNo, value).is_empty());
    }
    for value in [
        json!(0),
        json!(1),
        json!("yes"),
        json!("false"),
        Value::Null,
    ] {
        assert_eq!(
            reasons(FieldType::YesNo, value),
            vec![ValidationReason::WrongType]
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn text_counts_unicode_scalars_and_preserves_whitespace() {
    for kind in [short(2, 2), long(2, 2)] {
        for text in ["é🌲", "e\u{301}", "\n "] {
            assert!(reasons(kind.clone(), json!(text)).is_empty());
        }
        assert_eq!(
            reasons(kind.clone(), json!("🌲")),
            vec![ValidationReason::TextTooShort]
        );
        assert_eq!(
            reasons(kind.clone(), json!("abc")),
            vec![ValidationReason::TextTooLong]
        );
        assert_eq!(reasons(kind, json!(2)), vec![ValidationReason::WrongType]);
    }
    assert!(reasons(short(0, 0), json!("")).is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn location_requires_only_two_bounded_numeric_coordinates() {
    for value in [
        json!({"latitude": -90, "longitude": -180}),
        json!({"latitude": 90, "longitude": 180}),
        json!({"latitude": 0, "longitude": 0}),
    ] {
        assert!(reasons(FieldType::Location, value).is_empty());
    }
    for value in [
        json!({"latitude": 90.1, "longitude": 0}),
        json!({"latitude": 0, "longitude": -180.1}),
        json!({"latitude": "0", "longitude": 0}),
        json!({"latitude": 0}),
        json!({"latitude": 0, "longitude": 0, "address": "extra"}),
        json!([0, 0]),
        Value::Null,
    ] {
        assert_eq!(
            reasons(FieldType::Location, value),
            vec![ValidationReason::InvalidLocation]
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn required_and_optional_are_presence_rules_and_null_is_invalid() {
    let mut schema = schema_with(short(0, 10));
    let mut profile = profile_with(json!(""));
    assert!(
        schema
            .compile()
            .unwrap()
            .validate_profile(&profile)
            .is_valid()
    );
    profile.public.clear();
    assert_eq!(
        schema
            .compile()
            .unwrap()
            .validate_profile(&profile)
            .violations[0]
            .reason,
        ValidationReason::MissingRequired
    );
    schema.public[0].required = false;
    assert!(
        schema
            .compile()
            .unwrap()
            .validate_profile(&profile)
            .is_valid()
    );
    profile.public.insert("answer".into(), Value::Null);
    assert_eq!(
        schema
            .compile()
            .unwrap()
            .validate_profile(&profile)
            .violations[0]
            .reason,
        ValidationReason::WrongType
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn public_validation_does_not_require_private_answers() {
    let compiled = all_types().compile().unwrap();
    let mut profile = complete_profile();
    profile.private.clear();
    assert!(
        compiled
            .validate_public(&PublicProfile::from(&profile))
            .is_valid()
    );
    let issues = compiled.validate_profile(&profile).violations;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].part, Some(Part::Private));
    assert_eq!(issues[0].field.as_deref(), Some("about"));
    assert_eq!(issues[0].reason, ValidationReason::MissingRequired);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn misplaced_and_unknown_fields_are_rejected_without_echoing_them() {
    let compiled = all_types().compile().unwrap();
    let mut profile = complete_profile();
    profile
        .public
        .insert("about".into(), json!("private-text-canary"));
    profile
        .private
        .insert("unknown-key-canary".into(), json!("secret-value-canary"));
    profile
        .public
        .insert("age".into(), json!("invalid-age-canary"));
    let report = compiled.validate_profile(&profile);
    assert_eq!(report.violations.len(), 3);
    let wire = serde_json::to_string(&report).unwrap();
    let debug = format!("{report:?}");
    for canary in [
        "private-text-canary",
        "unknown-key-canary",
        "secret-value-canary",
        "invalid-age-canary",
    ] {
        assert!(!wire.contains(canary));
        assert!(!debug.contains(canary));
    }
    assert_eq!(report, serde_json::from_str(&wire).unwrap());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn community_and_revision_mismatches_short_circuit_content_checks() {
    let mut schema = schema_with(short(1, 40));
    schema.public[0].no_contact_details = true;
    let compiled = schema.compile().unwrap();
    let mut profile = profile_with(json!("hello"));
    profile.community = "another-community".into();
    profile.schema_version = 2;
    let report = compiled.validate_profile_with(&profile, &|_| panic!("must not check content"));
    assert_eq!(
        report
            .violations
            .iter()
            .map(|v| v.reason)
            .collect::<Vec<_>>(),
        vec![
            ValidationReason::CommunityMismatch,
            ValidationReason::VersionMismatch
        ]
    );
    assert_eq!(
        report,
        compiled.validate_public(&PublicProfile::from(&profile))
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn compiled_revision_is_independent_of_later_definition_edits() {
    let mut schema = schema_with(integer(0, 10));
    let compiled = schema.compile().unwrap();
    schema.public[0].kind = integer(100, 200);
    assert!(
        compiled
            .validate_profile(&profile_with(json!(5)))
            .is_valid()
    );
    assert!(
        !schema
            .compile()
            .unwrap()
            .validate_profile(&profile_with(json!(5)))
            .is_valid()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn contact_checks_fail_closed_and_apply_to_both_parts() {
    let mut schema = schema_with(short(1, 60));
    schema.public[0].no_contact_details = true;
    let mut private = field("private_text", long(1, 60));
    private.no_contact_details = true;
    schema.private.push(private);
    let compiled = schema.compile().unwrap();
    let mut profile = profile_with(json!("hello"));
    profile
        .private
        .insert("private_text".into(), json!("hello"));
    let report = compiled.validate_profile(&profile);
    assert_eq!(report.violations.len(), 2);
    assert!(
        report
            .violations
            .iter()
            .all(|v| v.reason == ValidationReason::ContactCheckRequired)
    );
    let calls = Cell::new(0);
    let checker = |text: &str| {
        calls.set(calls.get() + 1);
        if text.contains('@') {
            ContactCheck::Found
        } else {
            ContactCheck::Clear
        }
    };
    assert!(
        compiled
            .validate_profile_with(&profile, &checker)
            .is_valid()
    );
    assert_eq!(calls.get(), 2);
    profile
        .public
        .insert("answer".into(), json!("person@example.invalid"));
    let report = compiled.validate_public_with(&PublicProfile::from(&profile), &checker);
    assert_eq!(report.violations.len(), 1);
    assert_eq!(
        report.violations[0].reason,
        ValidationReason::ContactDetails
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn absent_unrestricted_and_oversized_text_do_not_invoke_contact_checker() {
    let mut schema = schema_with(short(1, 10));
    let checker = |_: &str| panic!("checker must not be called");
    assert!(
        schema
            .compile()
            .unwrap()
            .validate_profile_with(&profile_with(json!("hello")), &checker)
            .is_valid()
    );
    schema.public[0].no_contact_details = true;
    schema.public[0].required = false;
    let compiled = schema.compile().unwrap();
    let mut profile = profile_with(json!("far too long to validate"));
    assert!(
        !compiled
            .validate_profile_with(&profile, &checker)
            .is_valid()
    );
    profile.public.clear();
    assert!(
        compiled
            .validate_profile_with(&profile, &checker)
            .is_valid()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn empty_schema_accepts_empty_profiles_but_not_arbitrary_fields() {
    let mut schema = schema_with(FieldType::YesNo);
    schema.public.clear();
    let compiled = schema.compile().unwrap();
    let mut profile = profile_with(json!(true));
    assert!(!compiled.validate_profile(&profile).is_valid());
    profile.public.clear();
    assert!(compiled.validate_profile(&profile).is_valid());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn all_field_types_reject_null_and_wrong_container_values() {
    let schema = all_types();
    for field in schema.public.iter().chain(&schema.private) {
        for value in [Value::Null, json!([])] {
            assert!(!reasons(field.kind.clone(), value).is_empty());
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn hostile_exponents_are_rejected_before_exact_arithmetic() {
    for text in [
        "0.01e-9223372036854775807",
        "1e9223372036854775807",
        "1e129",
        "1e-129",
    ] {
        let value: Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            reasons(decimal(None, None), value.clone()),
            vec![ValidationReason::NumberLimit]
        );
        assert_eq!(
            reasons(integer(-10, 10), value.clone()),
            vec![ValidationReason::NumberLimit]
        );
        assert_eq!(
            reasons(FieldType::Location, json!({"latitude":value,"longitude":0})),
            vec![ValidationReason::InvalidLocation]
        );
        assert!(cshm::parse_number(text).is_err());
    }
    assert!(cshm::parse_number(&format!("1.{}", "1".repeat(128))).is_err());
    for text in ["1e128", "1e-128", "0", "-1.25e2"] {
        assert!(cshm::parse_number(text).is_ok());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn fractional_values_do_not_round_across_integral_bounds() {
    for (min, max, text, expected) in [
        (
            Some("9007199254740993"),
            None,
            "9007199254740992.9",
            Some(ValidationReason::BelowMinimum),
        ),
        (
            None,
            Some("-9007199254740993"),
            "-9007199254740992.9",
            Some(ValidationReason::AboveMaximum),
        ),
        (Some("9007199254740993"), None, "90071992547409930e-1", None),
        (
            None,
            Some("-9007199254740993"),
            "-90071992547409930e-1",
            None,
        ),
    ] {
        assert_eq!(
            reasons(decimal(min, max), serde_json::from_str(text).unwrap()),
            expected.into_iter().collect::<Vec<_>>()
        );
    }
    for (latitude, longitude) in [
        ("90.000000000000000001", "0"),
        ("0", "-180.000000000000000001"),
    ] {
        let value = serde_json::from_str(&format!(
            "{{\"latitude\":{latitude},\"longitude\":{longitude}}}"
        ))
        .unwrap();
        assert_eq!(
            reasons(FieldType::Location, value),
            vec![ValidationReason::InvalidLocation]
        );
    }
}
