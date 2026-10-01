//! Failure diagnostics and policy transitions use the real schema engine.
mod common;

use std::error::Error;

use common::*;
use cshm::{
    ChangeClass, ChangeError, ContactCheck, FieldType, Profile, classify_changes, parse_number,
};
use serde_json::json;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn change_diagnostics_preserve_error_causes_without_profile_values() {
    let previous = schema_with(FieldType::YesNo);
    let mut next = previous.clone();
    next.version = 2;
    let mut invalid = previous.clone();
    invalid.version = 0;
    for (error, message, has_source) in [
        (
            classify_changes(&invalid, &next).unwrap_err(),
            "previous schema is invalid",
            true,
        ),
        (
            classify_changes(&previous, &invalid).unwrap_err(),
            "next schema is invalid",
            true,
        ),
        (
            ChangeError::CommunityMismatch,
            "schemas belong to different communities",
            false,
        ),
        (
            classify_changes(&previous, &previous).unwrap_err(),
            "schema revision must increase",
            false,
        ),
    ] {
        assert_eq!(error.to_string(), message);
        assert_eq!(error.source().is_some(), has_source);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn optional_metadata_and_removed_contact_restrictions_do_not_tighten() {
    let mut previous = schema_with(short(1, 40));
    previous.public[0].required = false;
    previous.public[0].no_contact_details = true;
    let mut next = previous.clone();
    next.version = 2;
    next.public[0].label = "New label".into();
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::Loosen
    );
    next.public[0].no_contact_details = false;
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::Loosen
    );
    let profile = profile_with(json!("contact@example.invalid"));
    assert!(
        !previous
            .compile()
            .unwrap()
            .validate_profile(&profile)
            .is_valid()
    );
    let mut updated = profile;
    updated.schema_version = 2;
    assert!(
        next.compile()
            .unwrap()
            .validate_profile(&updated)
            .is_valid()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn decimal_scale_and_syntax_budget_errors_are_value_free() {
    for text in ["1.1e-128", "invalid-number-canary", " 1 ", "1e+"] {
        let error = parse_number(text).unwrap_err();
        assert_eq!(
            error.to_string(),
            "number exceeds the supported syntax or precision budget"
        );
        assert!(error.source().is_none());
        assert!(!format!("{error:?}").contains(text));
    }
    assert!(parse_number("1e-128").is_ok());
    assert!(parse_number("1e128").is_ok());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn answer_maps_reject_non_object_wire_values() {
    for invalid in [json!([]), json!(false), json!("answer-canary"), json!(null)] {
        let error = serde_json::from_value::<Profile>(json!({
            "community": "walking-club",
            "schema_version": 1,
            "public": invalid,
            "private": {}
        }))
        .unwrap_err();
        assert!(error.to_string().contains("object with distinct field identifiers"));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn empty_optional_fields_need_no_contact_check() {
    let mut schema = schema_with(short(1, 40));
    schema.public[0].required = false;
    schema.public[0].no_contact_details = true;
    let mut profile = profile_with(json!("hello"));
    profile.public.clear();
    let compiled = schema.compile().unwrap();
    assert!(compiled.validate_profile(&profile).is_valid());
    assert!(
        compiled
            .validate_profile_with(&profile, &|_| ContactCheck::Found)
            .is_valid()
    );
}
