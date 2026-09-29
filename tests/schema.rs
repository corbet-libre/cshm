//! Definition invariants and wire-format rejection cases.
mod common;

use common::*;
use cshm::{FieldType, Profile, PublicProfile, Schema, SchemaIssueKind};
use serde_json::json;

#[test]
fn invalid_definition_reports_all_independent_issues() {
    let mut schema = schema_with(choice(&["a", "a", " "]));
    schema.community = "\n ".into();
    schema.version = 0;
    schema.public[0].id = " ".into();
    schema.public[0].label = "".into();
    schema.public[0].no_contact_details = true;
    if let FieldType::Choice { options } = &mut schema.public[0].kind {
        options[0].label = "".into();
    }
    schema.private.push(schema.public[0].clone());
    let errors = schema.validate_definition().unwrap_err();
    for kind in [
        SchemaIssueKind::BlankCommunity,
        SchemaIssueKind::ZeroVersion,
        SchemaIssueKind::BlankFieldId,
        SchemaIssueKind::DuplicateFieldId,
        SchemaIssueKind::BlankLabel,
        SchemaIssueKind::ContactFlagOnNonText,
        SchemaIssueKind::BlankChoiceLabel,
        SchemaIssueKind::DuplicateChoiceId,
        SchemaIssueKind::BlankChoiceId,
    ] {
        assert!(
            errors.0.iter().any(|issue| issue.kind == kind),
            "missing {kind:?}"
        );
    }
    assert!(schema.compile().is_err());
    assert!(!errors.to_string().is_empty());
}

#[test]
fn all_inverted_bounds_and_empty_choices_are_rejected() {
    for kind in [
        integer(10, 9),
        decimal(Some("0.1000000000000000002"), Some("0.1000000000000000001")),
        short(2, 1),
        long(2, 1),
    ] {
        let errors = schema_with(kind).validate_definition().unwrap_err();
        assert_eq!(errors.0.len(), 1);
        assert_eq!(errors.0[0].kind, SchemaIssueKind::InvertedBounds);
    }
    assert_eq!(
        schema_with(choice(&[]))
            .validate_definition()
            .unwrap_err()
            .0[0]
            .kind,
        SchemaIssueKind::EmptyChoices
    );
}

#[test]
fn equal_bounds_unbounded_numbers_and_zero_text_length_are_valid() {
    for kind in [
        integer(i64::MAX, i64::MAX),
        integer(i64::MIN, i64::MIN),
        decimal(Some("0.1"), Some("0.1")),
        decimal(None, None),
        short(0, 0),
        long(0, 0),
    ] {
        assert!(schema_with(kind).compile().is_ok());
    }
}

#[test]
fn duplicate_field_ids_are_rejected_within_and_across_parts() {
    let mut schema = schema_with(FieldType::YesNo);
    schema.public.push(schema.public[0].clone());
    assert_eq!(
        schema.validate_definition().unwrap_err().0[0].kind,
        SchemaIssueKind::DuplicateFieldId
    );
    schema.private.push(schema.public.pop().unwrap());
    assert_eq!(
        schema.validate_definition().unwrap_err().0[0].kind,
        SchemaIssueKind::DuplicateFieldId
    );
}

#[test]
fn malformed_and_unsupported_schema_wire_data_is_rejected() {
    let base = serde_json::to_value(schema_with(short(0, 40))).unwrap();
    let mut variants = Vec::new();
    let mut value = base.clone();
    value["public"][0]["kind"] = json!({"type": "picture"});
    variants.push(value);
    let mut value = base.clone();
    value["public"][0]["kind"]["$ref"] = json!("https://example.invalid/schema");
    variants.push(value);
    let mut value = base.clone();
    value["public"][0]["kind"]["min_length"] = json!(-1);
    variants.push(value);
    let mut value = base.clone();
    value["public"][0]["change_preset"] = json!("sometimes");
    variants.push(value);
    let mut value = base.clone();
    value["extra"] = json!(true);
    variants.push(value);
    let mut value = base.clone();
    value["public"][0]["secret"] = json!(true);
    variants.push(value);
    let mut value = base;
    value["version"] = json!(4_294_967_296_u64);
    variants.push(value);
    for value in variants {
        assert!(serde_json::from_value::<Schema>(value).is_err());
    }
}

#[test]
fn decimal_bounds_must_use_exact_decimal_strings() {
    let base = serde_json::to_value(schema_with(decimal(Some("0.1"), None))).unwrap();
    for invalid in [
        json!(0.1),
        json!(""),
        json!("NaN"),
        json!("Infinity"),
        json!("not a decimal"),
        json!("0.12345678901234567890123456789"),
        json!("0.00000000000000000000000000001"),
        json!("79228162514264337593543950336"),
    ] {
        let mut value = base.clone();
        value["public"][0]["kind"]["min"] = invalid;
        assert!(serde_json::from_value::<Schema>(value).is_err());
    }
}

#[test]
fn omitted_decimal_bounds_and_exact_extremes_round_trip() {
    let mut value = serde_json::to_value(schema_with(decimal(None, None))).unwrap();
    let kind = value["public"][0]["kind"].as_object_mut().unwrap();
    kind.remove("min");
    kind.remove("max");
    let schema: Schema = serde_json::from_value(value).unwrap();
    assert_eq!(schema.public[0].kind, decimal(None, None));
    for bound in [
        "0.0000000000000000000000000001",
        "79228162514264337593543950335",
        "-79228162514264337593543950335",
    ] {
        let schema = schema_with(decimal(Some(bound), Some(bound)));
        let decoded: Schema =
            serde_json::from_str(&serde_json::to_string(&schema).unwrap()).unwrap();
        assert_eq!(decoded, schema);
        assert!(
            decoded
                .compile()
                .unwrap()
                .validate_profile(&profile_with(serde_json::from_str(bound).unwrap()))
                .is_valid()
        );
    }
}

#[test]
fn profiles_reject_unknown_envelope_fields_and_duplicate_answer_ids() {
    let duplicate_public = r#"{"community":"walking-club","schema_version":1,"public":{"answer":true,"answer":false},"private":{}}"#;
    let duplicate_private = r#"{"community":"walking-club","schema_version":1,"public":{},"private":{"answer":true,"answer":false}}"#;
    for wire in [duplicate_public, duplicate_private] {
        assert!(serde_json::from_str::<Profile>(wire).is_err());
    }
    let duplicate_projection = r#"{"community":"walking-club","schema_version":1,"public":{"answer":true,"answer":false}}"#;
    assert!(serde_json::from_str::<PublicProfile>(duplicate_projection).is_err());
    let mut value = serde_json::to_value(profile_with(json!(true))).unwrap();
    assert!(serde_json::from_value::<PublicProfile>(value.clone()).is_err());
    value["extra"] = json!(true);
    assert!(serde_json::from_value::<Profile>(value).is_err());
}

#[test]
fn unusual_identifiers_are_literal_keys_and_never_json_schema_paths() {
    let mut schema = schema_with(FieldType::YesNo);
    schema.public[0].id = "a/~b.$ref".into();
    let mut profile = profile_with(json!(true));
    profile.public.clear();
    profile.public.insert("a/~b.$ref".into(), json!(true));
    let compiled = schema.compile().unwrap();
    assert!(compiled.validate_profile(&profile).is_valid());
    profile.public.insert("a/~b.$ref".into(), json!(null));
    assert_eq!(
        compiled.validate_profile(&profile).violations[0]
            .field
            .as_deref(),
        Some("a/~b.$ref")
    );
}
