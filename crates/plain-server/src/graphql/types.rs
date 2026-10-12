use async_graphql::{Enum, ID, InputObject, SimpleObject};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Instant(pub chrono::DateTime<chrono::Utc>);

#[async_graphql::Scalar]
impl async_graphql::ScalarType for Instant {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        if let async_graphql::Value::String(s) = &value
            && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s)
        {
            return Ok(Instant(dt.with_timezone(&chrono::Utc)));
        }
        Err(async_graphql::InputValueError::expected_type(value))
    }

    fn to_value(&self) -> async_graphql::Value {
        async_graphql::Value::String(self.wire())
    }
}

impl Instant {
    fn wire(&self) -> String {
        self.0.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }
}
impl serde::Serialize for Instant {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire())
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ActionResult {
    #[graphql(name = "affectedCount")]
    pub affected_count: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize)]
#[serde(transparent)]
pub struct Long(pub i64);

#[async_graphql::Scalar]
impl async_graphql::ScalarType for Long {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        if let async_graphql::Value::Number(n) = &value
            && let Some(i) = n.as_i64()
        {
            return Ok(Long(i));
        }
        Err(async_graphql::InputValueError::expected_type(value))
    }

    fn to_value(&self) -> async_graphql::Value {
        async_graphql::Value::from(self.0)
    }
}

pub(crate) fn parse_instant(value: &str) -> async_graphql::Result<Instant> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| Instant(dt.with_timezone(&chrono::Utc)))
        .map_err(|e| async_graphql::Error::new(format!("invalid stored timestamp: {e}")))
}

/// The `app` root echoes a write back rather than reading the store again,
/// so a caller can confirm what it asked for without a round trip.
#[derive(SimpleObject, Clone, Debug)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
}

#[cfg(feature = "core")]
pub mod facts;
