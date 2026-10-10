use crate::graphql::types::Long;
use async_graphql::ID;
use async_graphql::InputObject;
use async_graphql::SimpleObject;

#[derive(SimpleObject, Clone, Debug)]
pub struct Tag {
    pub id: ID,
    pub name: String,
    /// Numeric tag kind — the ordinal of the tag's `DataType`
    /// (DEFAULT=0, AUDIO=1, VIDEO=2, IMAGE=3, …). The NAS only has data
    /// for the media kinds; the Int is frozen phone contract.
    pub r#type: i32,
    pub count: i32,
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
