use async_graphql::{ID, SimpleObject};

#[derive(SimpleObject, Clone, Debug)]
pub struct Tag {
    pub id: ID,
    pub name: String,
    pub count: i32,
}
