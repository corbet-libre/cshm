use std::collections::BTreeSet;
use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// The visibility boundary of a field or a validation issue.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    /// Visible to strangers and used in public matching.
    Public,
    /// Available only to accepted contacts; validated on devices.
    Private,
}

/// A change restriction, ordered from most restricted to least restricted.
///
/// Token quotas and pin enforcement belong to the caller, not this crate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangePreset {
    /// Intended to remain stable.
    Stable,
    /// May change occasionally.
    Occasional,
    /// May change freely.
    Free,
}

/// An option has a stable identifier distinct from its display label.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceOption {
    /// The string stored in a profile.
    pub id: String,
    /// The text shown to a member.
    pub label: String,
}

/// Supported answer types. Bounds are inclusive and text lengths count Unicode
/// scalar values. Pictures and arbitrary schema extensions are unsupported.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldType {
    /// Exactly one of the declared option identifiers.
    Choice {
        /// A nonempty list of distinct, nonblank identifiers and labels.
        options: Vec<ChoiceOption>,
    },
    /// A signed 64-bit integer; mathematical integral JSON numbers are accepted.
    Integer {
        /// Inclusive lower bound; absence means `i64::MIN`.
        min: Option<i64>,
        /// Inclusive upper bound; absence means `i64::MAX`.
        max: Option<i64>,
    },
    /// A JSON number, checked exactly rather than through binary floating point.
    /// Bounds use decimal strings on the schema wire, with up to 28 scale digits.
    Decimal {
        /// Inclusive lower bound, or unbounded.
        #[serde(default, with = "exact_decimal")]
        min: Option<Decimal>,
        /// Inclusive upper bound, or unbounded.
        #[serde(default, with = "exact_decimal")]
        max: Option<Decimal>,
    },
    /// A JSON boolean.
    YesNo,
    /// An object with numeric `latitude` (-90..=90) and `longitude` (-180..=180).
    Location,
    /// Text presented as a short answer; the schema sets its length limits.
    ShortText {
        /// Inclusive minimum length in Unicode scalar values.
        min_length: u32,
        /// Inclusive maximum length in Unicode scalar values.
        max_length: u32,
    },
    /// Text presented as a long answer; it uses the same length rules as short text.
    LongText {
        /// Inclusive minimum length in Unicode scalar values.
        min_length: u32,
        /// Inclusive maximum length in Unicode scalar values.
        max_length: u32,
    },
}

impl FieldType {
    pub(crate) fn is_text(&self) -> bool {
        matches!(self, Self::ShortText { .. } | Self::LongText { .. })
    }

    pub(crate) fn text_bounds(&self) -> Option<(u32, u32)> {
        match self {
            Self::ShortText {
                min_length,
                max_length,
            }
            | Self::LongText {
                min_length,
                max_length,
            } => Some((*min_length, *max_length)),
            _ => None,
        }
    }

    pub(crate) fn number_bounds(&self) -> Option<(Option<Decimal>, Option<Decimal>)> {
        match self {
            Self::Integer { min, max } => Some((
                Some(Decimal::from(min.unwrap_or(i64::MIN))),
                Some(Decimal::from(max.unwrap_or(i64::MAX))),
            )),
            Self::Decimal { min, max } => Some((*min, *max)),
            _ => None,
        }
    }
}

mod exact_decimal {
    use rust_decimal::Decimal;
    use serde::{Deserialize, Deserializer};

    pub use rust_decimal::serde::str_option::serialize;

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Decimal>, D::Error> {
        // The upstream default string deserializer may round excess precision
        // or turn an empty string into None. Neither is safe for a schema bound.
        Option::<String>::deserialize(deserializer)?
            .map(|value| {
                Decimal::from_str_exact(&value).map_err(|_| {
                    serde::de::Error::custom("decimal bound is not exactly representable")
                })
            })
            .transpose()
    }
}

/// One question in a community's profile form.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// Stable identifier, unique across both parts of a schema.
    pub id: String,
    /// Nonblank display label.
    pub label: String,
    /// The answer type and its constraints.
    pub kind: FieldType,
    /// Whether an answer must be present. Null never counts as an answer.
    pub required: bool,
    /// Whether upstream matching interfaces may offer a filter for this field.
    pub filterable: bool,
    /// Change restriction metadata; no quota or clock is kept here.
    pub change_preset: ChangePreset,
    /// Require a caller-supplied contact check for text. Invalid on other types.
    pub no_contact_details: bool,
}

/// One immutable-by-convention schema revision for one community.
///
/// Deserialization checks the wire shape. Call [`Schema::validate_definition`]
/// or [`Schema::compile`] to check semantic invariants before using a revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Schema {
    /// Opaque, nonblank community identifier; compared exactly.
    pub community: String,
    /// Nonzero revision. A successor must have a strictly larger revision.
    pub version: u32,
    /// Public fields, in presentation order.
    pub public: Vec<Field>,
    /// Private fields, in presentation order.
    pub private: Vec<Field>,
}

/// Why a schema definition cannot be used.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaIssueKind {
    /// The community identifier is blank.
    BlankCommunity,
    /// Revision zero is not a schema version.
    ZeroVersion,
    /// A field identifier is blank.
    BlankFieldId,
    /// A field identifier occurs more than once, possibly across parts.
    DuplicateFieldId,
    /// A field label is blank.
    BlankLabel,
    /// A choice has no options.
    EmptyChoices,
    /// A choice identifier is blank.
    BlankChoiceId,
    /// A choice label is blank.
    BlankChoiceLabel,
    /// A choice identifier occurs more than once.
    DuplicateChoiceId,
    /// The lower bound exceeds the upper bound.
    InvertedBounds,
    /// Contact checking was requested on a non-text field.
    ContactFlagOnNonText,
    /// An internally generated JSON Schema could not be compiled.
    BackendCompilation,
}

/// A schema issue located at a declared field, or at the schema itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaIssue {
    /// Affected part, if this is a field issue.
    pub part: Option<Part>,
    /// Declared field identifier, if applicable.
    pub field: Option<String>,
    /// Machine-readable explanation.
    pub kind: SchemaIssueKind,
}

/// All semantic issues found in a schema definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SchemaErrors(pub Vec<SchemaIssue>);

impl fmt::Display for SchemaErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "schema has {} definition issue(s)", self.0.len())
    }
}

impl std::error::Error for SchemaErrors {}

impl Schema {
    /// Check schema invariants without compiling a profile validator.
    pub fn validate_definition(&self) -> Result<(), SchemaErrors> {
        let mut issues = Vec::new();
        let mut report = |part, field, kind| {
            issues.push(SchemaIssue { part, field, kind });
        };
        if self.community.trim().is_empty() {
            report(None, None, SchemaIssueKind::BlankCommunity);
        }
        if self.version == 0 {
            report(None, None, SchemaIssueKind::ZeroVersion);
        }
        let mut ids = BTreeSet::new();
        for (part, field) in self.fields() {
            let mut report = |kind| report(Some(part), Some(field.id.clone()), kind);
            if field.id.trim().is_empty() {
                report(SchemaIssueKind::BlankFieldId);
            }
            if !ids.insert(&field.id) {
                report(SchemaIssueKind::DuplicateFieldId);
            }
            if field.label.trim().is_empty() {
                report(SchemaIssueKind::BlankLabel);
            }
            if field.no_contact_details && !field.kind.is_text() {
                report(SchemaIssueKind::ContactFlagOnNonText);
            }
            match &field.kind {
                FieldType::Choice { options } => {
                    if options.is_empty() {
                        report(SchemaIssueKind::EmptyChoices);
                    }
                    let mut choices = BTreeSet::new();
                    for option in options {
                        if option.id.trim().is_empty() {
                            report(SchemaIssueKind::BlankChoiceId);
                        }
                        if option.label.trim().is_empty() {
                            report(SchemaIssueKind::BlankChoiceLabel);
                        }
                        if !choices.insert(&option.id) {
                            report(SchemaIssueKind::DuplicateChoiceId);
                        }
                    }
                }
                kind => {
                    let inverted_number = kind.number_bounds().is_some_and(
                        |(min, max)| matches!((min, max), (Some(a), Some(b)) if a > b),
                    );
                    let inverted_text = kind.text_bounds().is_some_and(|(min, max)| min > max);
                    if inverted_number || inverted_text {
                        report(SchemaIssueKind::InvertedBounds);
                    }
                }
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(SchemaErrors(issues))
        }
    }

    pub(crate) fn fields(&self) -> impl Iterator<Item = (Part, &Field)> {
        self.public
            .iter()
            .map(|field| (Part::Public, field))
            .chain(self.private.iter().map(|field| (Part::Private, field)))
    }
}
