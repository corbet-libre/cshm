use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use jsonschema::error::ValidationErrorKind;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use crate::{Field, FieldType, Part, Schema, SchemaErrors, SchemaIssue, SchemaIssueKind};

/// Answers keyed by stable field identifiers. Values are ordinary JSON values.
pub type Answers = BTreeMap<String, Value>;

/// A complete profile for validation on a member's device.
///
/// Do not send this object to a service that may only see public answers.
/// Use [`PublicProfile`] at that boundary instead.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Community whose schema applies.
    pub community: String,
    /// Exact schema revision used by these answers.
    pub schema_version: u32,
    /// Public answers; duplicate keys on the JSON wire are rejected.
    #[serde(deserialize_with = "unique_answers")]
    pub public: Answers,
    /// Private answers; duplicate keys on the JSON wire are rejected.
    #[serde(deserialize_with = "unique_answers")]
    pub private: Answers,
}

/// A public profile projection, with no slot for private answers.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicProfile {
    /// Community whose schema applies.
    pub community: String,
    /// Exact schema revision used by these answers.
    pub schema_version: u32,
    /// Public answers; duplicate keys on the JSON wire are rejected.
    #[serde(deserialize_with = "unique_answers")]
    pub public: Answers,
}

impl From<&Profile> for PublicProfile {
    fn from(profile: &Profile) -> Self {
        Self {
            community: profile.community.clone(),
            schema_version: profile.schema_version,
            public: profile.public.clone(),
        }
    }
}

fn unique_answers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Answers, D::Error> {
    struct AnswersVisitor;

    impl<'de> Visitor<'de> for AnswersVisitor {
        type Value = Answers;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an object with distinct field identifiers")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Answers, A::Error> {
            let mut answers = Answers::new();
            while let Some((key, value)) = map.next_entry::<String, Value>()? {
                if answers.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate answer identifier"));
                }
            }
            Ok(answers)
        }
    }

    deserializer.deserialize_map(AnswersVisitor)
}

/// The result of the caller's check for contact details in a text answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactCheck {
    /// The caller's policy accepts this text.
    Clear,
    /// The caller detected prohibited contact details.
    Found,
    /// The check is unavailable or inconclusive; validation must not succeed.
    Unavailable,
}

/// Stable validation reasons; no reason contains an answer or an unknown key.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationReason {
    /// The profile belongs to a different community.
    CommunityMismatch,
    /// The profile targets a different schema revision.
    VersionMismatch,
    /// A required field has no answer.
    MissingRequired,
    /// One or more undeclared fields occur in this part.
    UnexpectedField,
    /// A value has the wrong JSON type, including null.
    WrongType,
    /// A choice answer is not a declared option identifier.
    UnknownChoice,
    /// A number is below its inclusive minimum.
    BelowMinimum,
    /// A number is above its inclusive maximum.
    AboveMaximum,
    /// A text answer is shorter than its minimum length.
    TextTooShort,
    /// A text answer is longer than its maximum length.
    TextTooLong,
    /// A location has missing/extra coordinates, wrong types, or invalid ranges.
    InvalidLocation,
    /// The caller detected contact details in restricted text.
    ContactDetails,
    /// Restricted text needs an available, conclusive caller-supplied check.
    ContactCheckRequired,
    /// Another generated constraint was not satisfied.
    ConstraintViolation,
}

/// A failure at a declared field, part, or the profile envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Violation {
    /// Affected part, or none for an envelope mismatch.
    pub part: Option<Part>,
    /// Declared field identifier, or none for an envelope or unknown-field issue.
    pub field: Option<String>,
    /// Machine-readable explanation, without submitted values.
    pub reason: ValidationReason,
}

/// All violations found in deterministic schema order. Empty means valid.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReport {
    /// Violations; profile values and undeclared field names are never included.
    pub violations: Vec<Violation>,
}

impl ValidationReport {
    /// Whether the requested validation passed every check.
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }

    fn push(&mut self, part: Option<Part>, field: Option<&str>, reason: ValidationReason) {
        self.violations.push(Violation {
            part,
            field: field.map(str::to_owned),
            reason,
        });
    }
}

struct CompiledField {
    part: Part,
    field: Field,
    validator: jsonschema::Validator,
}

/// An owned, checked schema and reusable upstream validators.
///
/// Compiling generates only a closed JSON Schema subset. There are no external
/// references, filesystem lookups, network calls, signatures or storage handles.
pub struct CompiledSchema {
    schema: Schema,
    fields: Vec<CompiledField>,
}

impl Schema {
    /// Check the definition and compile reusable validators for both parts.
    pub fn compile(&self) -> Result<CompiledSchema, SchemaErrors> {
        self.validate_definition()?;
        let mut fields = Vec::new();
        for (part, field) in self.fields() {
            let validator = jsonschema::draft202012::options()
                .build(&field_schema(&field.kind))
                .map_err(|_| {
                    SchemaErrors(vec![SchemaIssue {
                        part: Some(part),
                        field: Some(field.id.clone()),
                        kind: SchemaIssueKind::BackendCompilation,
                    }])
                })?;
            fields.push(CompiledField {
                part,
                field: field.clone(),
                validator,
            });
        }
        Ok(CompiledSchema {
            schema: self.clone(),
            fields,
        })
    }
}

impl CompiledSchema {
    /// The checked revision; callers cannot mutate it through this reference.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Validate both parts. Restricted text returns `ContactCheckRequired`.
    pub fn validate_profile(&self, profile: &Profile) -> ValidationReport {
        self.validate_profile_with(profile, &|_| ContactCheck::Unavailable)
    }

    /// Validate both parts with a contact checker for each present restricted
    /// text answer. The checker must return the same result on every platform.
    pub fn validate_profile_with(
        &self,
        profile: &Profile,
        check_contact: &impl Fn(&str) -> ContactCheck,
    ) -> ValidationReport {
        let mut report = self.check_envelope(&profile.community, profile.schema_version);
        if report.is_valid() {
            self.check_part(Part::Public, &profile.public, check_contact, &mut report);
            self.check_part(Part::Private, &profile.private, check_contact, &mut report);
        }
        report
    }

    /// Validate only public answers. Success says nothing about private answers.
    pub fn validate_public(&self, profile: &PublicProfile) -> ValidationReport {
        self.validate_public_with(profile, &|_| ContactCheck::Unavailable)
    }

    /// Validate a public projection using the caller's contact checker.
    pub fn validate_public_with(
        &self,
        profile: &PublicProfile,
        check_contact: &impl Fn(&str) -> ContactCheck,
    ) -> ValidationReport {
        let mut report = self.check_envelope(&profile.community, profile.schema_version);
        if report.is_valid() {
            self.check_part(Part::Public, &profile.public, check_contact, &mut report);
        }
        report
    }

    fn check_envelope(&self, community: &str, version: u32) -> ValidationReport {
        let mut report = ValidationReport::default();
        if community != self.schema.community {
            report.push(None, None, ValidationReason::CommunityMismatch);
        }
        if version != self.schema.version {
            report.push(None, None, ValidationReason::VersionMismatch);
        }
        report
    }

    fn check_part(
        &self,
        part: Part,
        answers: &Answers,
        check_contact: &impl Fn(&str) -> ContactCheck,
        report: &mut ValidationReport,
    ) {
        let fields = match part {
            Part::Public => &self.schema.public,
            Part::Private => &self.schema.private,
        };
        let known: BTreeSet<_> = fields.iter().map(|field| field.id.as_str()).collect();
        if answers.keys().any(|key| !known.contains(key.as_str())) {
            report.push(Some(part), None, ValidationReason::UnexpectedField);
        }
        for compiled in self.fields.iter().filter(|field| field.part == part) {
            let field = &compiled.field;
            let Some(value) = answers.get(&field.id) else {
                if field.required {
                    report.push(
                        Some(part),
                        Some(&field.id),
                        ValidationReason::MissingRequired,
                    );
                }
                continue;
            };
            let mut reasons = BTreeSet::new();
            for error in compiled.validator.iter_errors(value) {
                let reason = if field.kind == FieldType::Location {
                    ValidationReason::InvalidLocation
                } else {
                    match error.kind() {
                        ValidationErrorKind::Type { .. } => ValidationReason::WrongType,
                        ValidationErrorKind::Enum { .. } => ValidationReason::UnknownChoice,
                        ValidationErrorKind::Minimum { .. } => ValidationReason::BelowMinimum,
                        ValidationErrorKind::Maximum { .. } => ValidationReason::AboveMaximum,
                        ValidationErrorKind::MinLength { .. } => ValidationReason::TextTooShort,
                        ValidationErrorKind::MaxLength { .. } => ValidationReason::TextTooLong,
                        _ => ValidationReason::ConstraintViolation,
                    }
                };
                reasons.insert(reason);
            }
            // Avoid calling a content checker on structurally invalid or oversized text.
            if reasons.is_empty()
                && field.no_contact_details
                && let Some(text) = value.as_str()
            {
                match check_contact(text) {
                    ContactCheck::Clear => {}
                    ContactCheck::Found => {
                        reasons.insert(ValidationReason::ContactDetails);
                    }
                    ContactCheck::Unavailable => {
                        reasons.insert(ValidationReason::ContactCheckRequired);
                    }
                }
            }
            for reason in reasons {
                report.push(Some(part), Some(&field.id), reason);
            }
        }
    }
}

fn field_schema(kind: &FieldType) -> Value {
    match kind {
        FieldType::Choice { options } => json!({
            "type": "string",
            "enum": options.iter().map(|option| &option.id).collect::<Vec<_>>()
        }),
        FieldType::Integer { min, max } => json!({
            "type": "integer",
            "minimum": min.unwrap_or(i64::MIN),
            "maximum": max.unwrap_or(i64::MAX)
        }),
        FieldType::Decimal { min, max } => {
            let mut schema = json!({"type": "number"});
            for (key, bound) in [("minimum", min), ("maximum", max)] {
                if let Some(bound) = bound {
                    // Decimal's display is valid JSON numeric syntax. Parsing
                    // preserves all digits with the arbitrary_precision feature.
                    schema[key] = Value::Number(bound.to_string().parse().expect("decimal number"));
                }
            }
            schema
        }
        FieldType::YesNo => json!({"type": "boolean"}),
        FieldType::Location => json!({
            "type": "object",
            "properties": {
                "latitude": {"type": "number", "minimum": -90, "maximum": 90},
                "longitude": {"type": "number", "minimum": -180, "maximum": 180}
            },
            "required": ["latitude", "longitude"],
            "additionalProperties": false
        }),
        FieldType::ShortText {
            min_length,
            max_length,
        }
        | FieldType::LongText {
            min_length,
            max_length,
        } => json!({"type": "string", "minLength": min_length, "maxLength": max_length}),
    }
}
