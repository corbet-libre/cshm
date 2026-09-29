use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Field, FieldType, Part, Schema, SchemaErrors};

/// The public consequence of a field edit. Mixed edits are classified
/// conservatively: any tightening wins unless the public field is hidden.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeClass {
    /// Remove the old public value from public entries immediately. This also
    /// covers a move to private; it does not copy or disclose an answer.
    Hide,
    /// Existing answers remain acceptable without member input. Includes edits
    /// to presentation and filter metadata that do not narrow the answer set.
    Loosen,
    /// Some members may need to supply or approve an answer. The operator,
    /// outside this crate, chooses strict enforcement or grandfathering.
    Tighten,
    /// Only private fields are affected; clients apply the new definition.
    PrivateOnly,
}

/// One aspect of a field definition that changed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeReason {
    /// A field was introduced.
    Added,
    /// A field was removed.
    Removed,
    /// A field moved between the public and private parts.
    Visibility,
    /// The answer type, options, or bounds changed.
    Definition,
    /// The requirement to supply an answer changed.
    Required,
    /// The restriction on future changes changed.
    ChangePreset,
    /// The requirement to check text for contact details changed.
    ContactPolicy,
    /// Display label or filterability changed.
    Metadata,
}

/// A changed field, identified by its stable ID rather than its position.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldChange {
    /// Stable field identifier.
    pub field: String,
    /// Old visibility, or none for an addition.
    pub before: Option<Part>,
    /// New visibility, or none for a removal.
    pub after: Option<Part>,
    /// The conservative public consequence of the complete field edit.
    pub class: ChangeClass,
    /// Aspects that changed, in a stable order.
    pub reasons: Vec<ChangeReason>,
}

/// Changes between two checked revisions of the same community schema.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSet {
    /// Community whose schema changed.
    pub community: String,
    /// Previous schema revision.
    pub from_version: u32,
    /// Next schema revision (need not be consecutive).
    pub to_version: u32,
    /// Changed fields sorted by identifier; unchanged and reordered fields are omitted.
    pub fields: Vec<FieldChange>,
}

/// Why two schema revisions cannot be compared.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChangeError {
    /// The previous schema violates semantic invariants.
    InvalidPrevious(SchemaErrors),
    /// The next schema violates semantic invariants.
    InvalidNext(SchemaErrors),
    /// The schemas belong to different communities.
    CommunityMismatch,
    /// The next revision does not strictly exceed the previous revision.
    VersionNotIncreasing,
}

impl fmt::Display for ChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPrevious(_) => "previous schema is invalid",
            Self::InvalidNext(_) => "next schema is invalid",
            Self::CommunityMismatch => "schemas belong to different communities",
            Self::VersionNotIncreasing => "schema revision must increase",
        })
    }
}

impl std::error::Error for ChangeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidPrevious(errors) | Self::InvalidNext(errors) => Some(errors),
            _ => None,
        }
    }
}

/// Classify every edited field between increasing revisions of one community.
///
/// A private-to-public move always tightens, even for an optional field: an old
/// private answer must never become public without member input. No profiles
/// are migrated and no grandfathering or signing decision is made here.
pub fn classify_changes(previous: &Schema, next: &Schema) -> Result<ChangeSet, ChangeError> {
    previous
        .validate_definition()
        .map_err(ChangeError::InvalidPrevious)?;
    next.validate_definition()
        .map_err(ChangeError::InvalidNext)?;
    if previous.community != next.community {
        return Err(ChangeError::CommunityMismatch);
    }
    if next.version <= previous.version {
        return Err(ChangeError::VersionNotIncreasing);
    }
    let before: BTreeMap<_, _> = previous
        .fields()
        .map(|(part, field)| (&field.id, (part, field)))
        .collect();
    let after: BTreeMap<_, _> = next
        .fields()
        .map(|(part, field)| (&field.id, (part, field)))
        .collect();
    let mut fields = BTreeMap::new();
    for (id, &(part, field)) in &before {
        match after.get(id) {
            None => {
                fields.insert(
                    (*id).clone(),
                    FieldChange {
                        field: (*id).clone(),
                        before: Some(part),
                        after: None,
                        class: if part == Part::Public {
                            ChangeClass::Hide
                        } else {
                            ChangeClass::PrivateOnly
                        },
                        reasons: vec![ChangeReason::Removed],
                    },
                );
            }
            Some(&(new_part, new_field)) if part != new_part || field != new_field => {
                fields.insert(
                    (*id).clone(),
                    compare_field(part, field, new_part, new_field),
                );
            }
            _ => {}
        }
    }
    for (id, &(part, field)) in &after {
        if !before.contains_key(id) {
            fields.insert(
                (*id).clone(),
                FieldChange {
                    field: (*id).clone(),
                    before: None,
                    after: Some(part),
                    class: match part {
                        Part::Private => ChangeClass::PrivateOnly,
                        Part::Public if field.required => ChangeClass::Tighten,
                        Part::Public => ChangeClass::Loosen,
                    },
                    reasons: vec![ChangeReason::Added],
                },
            );
        }
    }
    Ok(ChangeSet {
        community: next.community.clone(),
        from_version: previous.version,
        to_version: next.version,
        fields: fields.into_values().collect(),
    })
}

fn compare_field(before: Part, old: &Field, after: Part, new: &Field) -> FieldChange {
    let mut reasons = Vec::new();
    for (changed, reason) in [
        (before != after, ChangeReason::Visibility),
        (old.kind != new.kind, ChangeReason::Definition),
        (old.required != new.required, ChangeReason::Required),
        (
            old.change_preset != new.change_preset,
            ChangeReason::ChangePreset,
        ),
        (
            old.no_contact_details != new.no_contact_details,
            ChangeReason::ContactPolicy,
        ),
        (
            old.label != new.label || old.filterable != new.filterable,
            ChangeReason::Metadata,
        ),
    ] {
        if changed {
            reasons.push(reason);
        }
    }
    let class = match (before, after) {
        (Part::Private, Part::Private) => ChangeClass::PrivateOnly,
        (Part::Public, Part::Private) => ChangeClass::Hide,
        (Part::Private, Part::Public) => ChangeClass::Tighten,
        (Part::Public, Part::Public) => {
            let tightens = (!old.required && new.required)
                || new.change_preset < old.change_preset
                || (!old.no_contact_details && new.no_contact_details)
                || !accepts_all(&new.kind, &old.kind);
            if tightens {
                ChangeClass::Tighten
            } else {
                ChangeClass::Loosen
            }
        }
    };
    FieldChange {
        field: old.id.clone(),
        before: Some(before),
        after: Some(after),
        class,
        reasons,
    }
}

fn accepts_all(new: &FieldType, old: &FieldType) -> bool {
    if new == old {
        return true;
    }
    if let (Some((new_min, new_max)), Some((old_min, old_max))) =
        (new.text_bounds(), old.text_bounds())
    {
        return new_min <= old_min && new_max >= old_max;
    }
    if let (Some((new_min, new_max)), Some((old_min, old_max))) =
        (new.number_bounds(), old.number_bounds())
    {
        // Decimal-to-integer changes may require member input even when both
        // numerical intervals match. Integral old values are safe to widen.
        if matches!(new, FieldType::Integer { .. }) && matches!(old, FieldType::Decimal { .. }) {
            return false;
        }
        return (new_min.is_none() || matches!((new_min, old_min), (Some(n), Some(o)) if n <= o))
            && (new_max.is_none() || matches!((new_max, old_max), (Some(n), Some(o)) if n >= o));
    }
    match (new, old) {
        (FieldType::Choice { options: new }, FieldType::Choice { options: old }) => {
            old.iter().all(|old| new.iter().any(|new| new.id == old.id))
        }
        _ => false,
    }
}
