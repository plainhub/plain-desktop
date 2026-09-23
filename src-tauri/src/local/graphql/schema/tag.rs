//! `tags` query — schema parity with `plain-app` `web/schemas/TagGraphQL.kt`.
//!
//! Always returns an empty list in local mode: the desktop local server
//! has no tag / tag-relation tables yet. The query exists only so the
//! popup lightbox's `tagsGQL` request resolves a valid schema instead
//! of failing with `Unknown field "tags"`.

use async_graphql::Object;

use crate::local::enums::DataType;
use crate::local::graphql::schema::types::Tag;

#[derive(Default)]
pub struct TagQuery;

#[Object]
impl TagQuery {
    async fn tags(&self, _type: DataType) -> Vec<Tag> {
        Vec::new()
    }
}
