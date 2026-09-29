//! Per-field change rules and validation compatibility.
mod common;

use common::*;
use cshm::{
    ChangeClass, ChangeError, ChangePreset, ChangeReason, ChangeSet, FieldType, Part,
    classify_changes,
};
use serde_json::json;

fn classify(old: FieldType, new: FieldType) -> ChangeClass {
    let previous = schema_with(old);
    let mut next = schema_with(new);
    next.version = 2;
    classify_changes(&previous, &next).unwrap().fields[0].class
}

#[test]
fn public_additions_removals_and_requiredness_follow_the_contract() {
    let previous = schema_with(FieldType::YesNo);
    let mut next = previous.clone();
    next.version = 2;
    next.public.clear();
    let change = &classify_changes(&previous, &next).unwrap().fields[0];
    assert_eq!(change.class, ChangeClass::Hide);
    assert_eq!(change.before, Some(Part::Public));
    assert_eq!(change.after, None);
    assert_eq!(change.reasons, vec![ChangeReason::Removed]);
    let mut empty = next.clone();
    empty.version = 1;
    next.public = previous.public.clone();
    assert_eq!(
        classify_changes(&empty, &next).unwrap().fields[0].class,
        ChangeClass::Tighten
    );
    next.public[0].required = false;
    assert_eq!(
        classify_changes(&empty, &next).unwrap().fields[0].class,
        ChangeClass::Loosen
    );
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::Loosen
    );
    let mut optional = next.clone();
    optional.version = 1;
    next.public[0].required = true;
    assert_eq!(
        classify_changes(&optional, &next).unwrap().fields[0].class,
        ChangeClass::Tighten
    );
}

#[test]
fn visibility_changes_never_publish_private_answers_implicitly() {
    let public = schema_with(FieldType::YesNo);
    let mut private = public.clone();
    private.version = 2;
    private.private = std::mem::take(&mut private.public);
    private.private[0].kind = short(5, 10);
    let changes = classify_changes(&public, &private).unwrap();
    assert_eq!(changes.fields[0].class, ChangeClass::Hide);
    assert_eq!(changes.fields[0].after, Some(Part::Private));
    assert!(
        changes.fields[0]
            .reasons
            .contains(&ChangeReason::Visibility)
    );
    let mut published = public;
    published.version = 3;
    published.public[0].required = false;
    assert_eq!(
        classify_changes(&private, &published).unwrap().fields[0].class,
        ChangeClass::Tighten
    );
}

#[test]
fn private_only_edits_additions_and_removals_stay_private() {
    let mut previous = schema_with(short(1, 100));
    previous.private = std::mem::take(&mut previous.public);
    let mut next = previous.clone();
    next.version = 2;
    next.private[0].kind = short(50, 60);
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::PrivateOnly
    );
    next.private.clear();
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::PrivateOnly
    );
    previous.private.clear();
    next.private.push(field("new", FieldType::YesNo));
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::PrivateOnly
    );
}

#[test]
fn widening_and_narrowing_numbers_and_text_are_classified() {
    for (old, new, expected) in [
        (integer(1, 10), integer(0, 11), ChangeClass::Loosen),
        (integer(1, 10), integer(2, 10), ChangeClass::Tighten),
        (integer(1, 10), integer(0, 9), ChangeClass::Tighten),
        (
            decimal(Some("0.1"), Some("1")),
            decimal(None, None),
            ChangeClass::Loosen,
        ),
        (
            decimal(None, None),
            decimal(Some("0.1"), None),
            ChangeClass::Tighten,
        ),
        (
            decimal(None, None),
            decimal(None, Some("1")),
            ChangeClass::Tighten,
        ),
        (
            decimal(Some("0.1"), Some("1")),
            decimal(Some("0.1000000000000000001"), Some("2")),
            ChangeClass::Tighten,
        ),
        (short(1, 10), short(0, 20), ChangeClass::Loosen),
        (short(1, 10), long(1, 10), ChangeClass::Loosen),
        (long(1, 10), short(1, 10), ChangeClass::Loosen),
        (long(1, 10), short(0, 9), ChangeClass::Tighten),
        (short(1, 10), short(2, 20), ChangeClass::Tighten),
    ] {
        assert_eq!(classify(old, new), expected);
    }
}

#[test]
fn numeric_representation_changes_are_conservative() {
    for (old, new, expected) in [
        (
            integer(1, 10),
            decimal(Some("1"), Some("10")),
            ChangeClass::Loosen,
        ),
        (integer(1, 10), decimal(None, None), ChangeClass::Loosen),
        (
            integer(1, 10),
            decimal(Some("1.1"), Some("10")),
            ChangeClass::Tighten,
        ),
        (
            decimal(Some("1"), Some("10")),
            integer(1, 10),
            ChangeClass::Tighten,
        ),
        (
            FieldType::Integer {
                min: None,
                max: None,
            },
            integer(i64::MIN, i64::MAX),
            ChangeClass::Loosen,
        ),
        (FieldType::YesNo, short(0, 100), ChangeClass::Tighten),
        (choice(&["a"]), short(0, 100), ChangeClass::Tighten),
    ] {
        assert_eq!(classify(old, new), expected);
    }
}

#[test]
fn choice_additions_removals_and_presentation_changes_use_stable_ids() {
    assert_eq!(
        classify(choice(&["a"]), choice(&["a", "b"])),
        ChangeClass::Loosen
    );
    assert_eq!(
        classify(choice(&["a", "b"]), choice(&["a"])),
        ChangeClass::Tighten
    );
    assert_eq!(
        classify(choice(&["a", "b"]), choice(&["a", "c"])),
        ChangeClass::Tighten
    );
    assert_eq!(
        classify(choice(&["a", "b"]), choice(&["b", "a"])),
        ChangeClass::Loosen
    );
    let mut renamed = choice(&["a"]);
    if let FieldType::Choice { options } = &mut renamed {
        options[0].label = "New label".into();
    }
    assert_eq!(classify(choice(&["a"]), renamed), ChangeClass::Loosen);
}

#[test]
fn metadata_restrictions_and_mixed_changes_are_explicit() {
    let previous = schema_with(short(1, 100));
    let mut next = previous.clone();
    next.version = 2;
    next.public[0].label = "New label".into();
    next.public[0].filterable = false;
    let changes = classify_changes(&previous, &next).unwrap();
    assert_eq!(changes.fields[0].class, ChangeClass::Loosen);
    assert_eq!(changes.fields[0].reasons, vec![ChangeReason::Metadata]);
    next.public[0].kind = short(0, 200);
    next.public[0].no_contact_details = true;
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::Tighten
    );
    next.public[0].no_contact_details = false;
    next.public[0].change_preset = ChangePreset::Stable;
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::Tighten
    );
    next.public[0].change_preset = ChangePreset::Free;
    assert_eq!(
        classify_changes(&previous, &next).unwrap().fields[0].class,
        ChangeClass::Loosen
    );
}

#[test]
fn every_preset_transition_has_the_expected_restriction_order() {
    let presets = [
        ChangePreset::Stable,
        ChangePreset::Occasional,
        ChangePreset::Free,
    ];
    for (old_rank, old) in presets.iter().enumerate() {
        for (new_rank, new) in presets.iter().enumerate() {
            let mut previous = schema_with(FieldType::YesNo);
            previous.public[0].change_preset = *old;
            let mut next = previous.clone();
            next.version = 2;
            next.public[0].change_preset = *new;
            let changes = classify_changes(&previous, &next).unwrap();
            if old_rank == new_rank {
                assert!(changes.fields.is_empty());
            } else {
                assert_eq!(
                    changes.fields[0].class,
                    if new_rank < old_rank {
                        ChangeClass::Tighten
                    } else {
                        ChangeClass::Loosen
                    }
                );
            }
        }
    }
}

#[test]
fn version_only_and_field_reordering_produce_no_field_changes() {
    let previous = all_types();
    let mut next = previous.clone();
    next.version += 3;
    next.public.reverse();
    assert!(
        classify_changes(&previous, &next)
            .unwrap()
            .fields
            .is_empty()
    );
}

#[test]
fn field_renaming_is_removal_and_addition_and_changes_round_trip() {
    let previous = schema_with(FieldType::YesNo);
    let mut next = previous.clone();
    next.version = 2;
    next.public[0].id = "new-id".into();
    let changes = classify_changes(&previous, &next).unwrap();
    assert_eq!(changes.fields.len(), 2);
    assert_eq!(changes.fields[0].field, "answer");
    assert_eq!(changes.fields[0].class, ChangeClass::Hide);
    assert_eq!(changes.fields[1].field, "new-id");
    assert_eq!(changes.fields[1].class, ChangeClass::Tighten);
    let decoded: ChangeSet =
        serde_json::from_str(&serde_json::to_string(&changes).unwrap()).unwrap();
    assert_eq!(decoded, changes);
}

#[test]
fn invalid_definitions_wrong_community_and_nonincreasing_versions_fail() {
    let previous = schema_with(FieldType::YesNo);
    let mut next = previous.clone();
    assert_eq!(
        classify_changes(&previous, &next),
        Err(ChangeError::VersionNotIncreasing)
    );
    next.version = 2;
    assert_eq!(
        classify_changes(&next, &previous),
        Err(ChangeError::VersionNotIncreasing)
    );
    next.community = "other".into();
    assert_eq!(
        classify_changes(&previous, &next),
        Err(ChangeError::CommunityMismatch)
    );
    next.community = previous.community.clone();
    next.public[0].kind = integer(5, 1);
    assert!(matches!(
        classify_changes(&previous, &next),
        Err(ChangeError::InvalidNext(_))
    ));
    assert!(matches!(
        classify_changes(&next, &previous),
        Err(ChangeError::InvalidPrevious(_))
    ));
}

#[test]
fn loosened_integer_intervals_preserve_previously_valid_answers() {
    // Exercise the classification against the real validator, including intervals
    // that widen on one end and narrow on the other.
    for old_min in -2..=2 {
        for old_max in old_min..=2 {
            let previous = schema_with(integer(old_min, old_max));
            let old_validator = previous.compile().unwrap();
            for new_min in -3..=3 {
                for new_max in new_min..=3 {
                    let mut next = schema_with(integer(new_min, new_max));
                    next.version = 2;
                    let changes = classify_changes(&previous, &next).unwrap();
                    let narrows = new_min > old_min || new_max < old_max;
                    if narrows {
                        assert_eq!(changes.fields[0].class, ChangeClass::Tighten);
                        continue;
                    }
                    assert!(
                        changes
                            .fields
                            .iter()
                            .all(|c| c.class == ChangeClass::Loosen)
                    );
                    let new_validator = next.compile().unwrap();
                    for value in -3..=3 {
                        let mut profile = profile_with(json!(value));
                        if old_validator.validate_profile(&profile).is_valid() {
                            profile.schema_version = 2;
                            assert!(new_validator.validate_profile(&profile).is_valid());
                        }
                    }
                }
            }
        }
    }
}
