use super::content_common::instant as parse_instant;
use super::types::Instant;
use crate::db::bookmark::{DBookmark, DBookmarkGroup};
use async_graphql::{ComplexObject, ID, InputObject, SimpleObject};

#[derive(SimpleObject)]
#[graphql(complex)]
#[graphql(name = "Bookmark")]
pub struct Bookmark {
    pub id: ID,
    pub url: String,
    pub title: String,
    pub favicon_path: String,
    pub group_id: ID,
    pub pinned: bool,
    pub click_count: i32,
    #[graphql(skip)]
    pub last_clicked_at: Option<String>,
    pub sort_order: i32,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

impl From<DBookmark> for Bookmark {
    fn from(b: DBookmark) -> Self {
        Self {
            id: b.id.into(),
            url: b.url,
            title: b.title,
            favicon_path: b.favicon_path,
            group_id: b.group_id.into(),
            pinned: b.pinned,
            click_count: b.click_count,
            last_clicked_at: b.last_clicked_at,
            sort_order: b.sort_order,
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}

#[derive(SimpleObject)]
#[graphql(complex)]
#[graphql(name = "BookmarkGroup")]
pub struct BookmarkGroup {
    pub id: ID,
    pub name: String,
    pub collapsed: bool,
    pub sort_order: i32,
    pub item_count: i32,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

impl BookmarkGroup {
    pub fn from_group(g: DBookmarkGroup, item_count: i32) -> Self {
        Self {
            id: g.id.into(),
            name: g.name,
            collapsed: g.collapsed,
            sort_order: g.sort_order,
            item_count,
            created_at: g.created_at,
            updated_at: g.updated_at,
        }
    }
}

#[derive(InputObject)]
#[graphql(name = "BookmarkInput")]
pub struct BookmarkInput {
    pub url: String,
    pub title: String,
    pub group_id: ID,
    pub pinned: bool,
    pub sort_order: i32,
}

#[ComplexObject]
impl Bookmark {
    async fn created_at(&self) -> async_graphql::Result<Instant> {
        parse_instant(&self.created_at)
    }

    async fn updated_at(&self) -> async_graphql::Result<Instant> {
        parse_instant(&self.updated_at)
    }

    async fn last_clicked_at(&self) -> async_graphql::Result<Option<Instant>> {
        self.last_clicked_at
            .as_deref()
            .map(parse_instant)
            .transpose()
    }
}

#[ComplexObject]
impl BookmarkGroup {
    async fn created_at(&self) -> async_graphql::Result<Instant> {
        parse_instant(&self.created_at)
    }
    async fn updated_at(&self) -> async_graphql::Result<Instant> {
        parse_instant(&self.updated_at)
    }
}
