#![allow(dead_code)]

use cshm::{ChangePreset, ChoiceOption, Field, FieldType, Profile, Schema};
use serde_json::{Value, json};

pub fn field(id: &str, kind: FieldType) -> Field {
    Field {
        id: id.into(),
        label: id.into(),
        kind,
        required: true,
        filterable: true,
        change_preset: ChangePreset::Occasional,
        no_contact_details: false,
    }
}

pub fn choice(ids: &[&str]) -> FieldType {
    FieldType::Choice {
        options: ids
            .iter()
            .map(|id| ChoiceOption {
                id: (*id).into(),
                label: (*id).into(),
            })
            .collect(),
    }
}

pub fn integer(min: i64, max: i64) -> FieldType {
    FieldType::Integer {
        min: Some(min),
        max: Some(max),
    }
}

pub fn decimal(min: Option<&str>, max: Option<&str>) -> FieldType {
    FieldType::Decimal {
        min: min.map(|n| n.parse().unwrap()),
        max: max.map(|n| n.parse().unwrap()),
    }
}

pub fn short(min_length: u32, max_length: u32) -> FieldType {
    FieldType::ShortText {
        min_length,
        max_length,
    }
}

pub fn long(min_length: u32, max_length: u32) -> FieldType {
    FieldType::LongText {
        min_length,
        max_length,
    }
}

pub fn schema_with(kind: FieldType) -> Schema {
    Schema {
        community: "walking-club".into(),
        version: 1,
        public: vec![field("answer", kind)],
        private: vec![],
    }
}

pub fn profile_with(value: Value) -> Profile {
    Profile {
        community: "walking-club".into(),
        schema_version: 1,
        public: [("answer".into(), value)].into(),
        private: Default::default(),
    }
}

pub fn all_types() -> Schema {
    Schema {
        community: "walking-club".into(),
        version: 7,
        public: vec![
            field("terrain", choice(&["forest", "mountain"])),
            field("age", integer(18, 120)),
            field(
                "distance",
                decimal(Some("0.1"), Some("100.0000000000000000001")),
            ),
            field("weekends", FieldType::YesNo),
            field("meeting", FieldType::Location),
            field("intro", short(1, 40)),
        ],
        private: vec![field("about", long(1, 2000))],
    }
}

pub fn complete_profile() -> Profile {
    serde_json::from_value(json!({
        "community": "walking-club",
        "schema_version": 7,
        "public": {
            "terrain": "forest", "age": 34, "distance": 12.5, "weekends": true,
            "meeting": {"latitude": 47.3, "longitude": 8.5}, "intro": "Walking together 🌲"
        },
        "private": {"about": "Long walks and good conversation.\nAll seasons."}
    }))
    .unwrap()
}
