use async_graphql::{ID, InputObject, SimpleObject};

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
        async_graphql::Value::String(self.0.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Tag {
    pub id: ID,
    pub name: String,
    /// Numeric tag kind — the ordinal of the tag's `DataType`
    /// (DEFAULT=0, AUDIO=1, VIDEO=2, IMAGE=3, …). The NAS only has data
    /// for the media kinds; the Int is frozen phone contract (API_SPEC
    /// §9), the mapping lives in docs/api/tags.md.
    pub r#type: i32,
    pub count: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ActionResult {
    #[graphql(name = "affectedCount")]
    pub affected_count: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
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

#[derive(SimpleObject, InputObject, Clone, Debug)]
pub struct TagRelationStub {
    pub key: String,
    pub title: String,
    pub size: Long,
}

/// One (tag, item-key) relation (plain-app contract). `key` is the media id
/// / entity key the tag is attached to.
#[derive(SimpleObject, Clone, Debug)]
pub struct TagRelation {
    #[graphql(name = "tagId")]
    pub tag_id: ID,
    pub key: String,
}
