use crate::assembly::db::DChannel;
use crate::assembly::db::DChat;
use crate::assembly::enums::ChannelStatus;
use crate::assembly::enums::ChatStatus;
use crate::assembly::enums::MemberStatus;
use crate::graphql::types::Long;
use async_graphql::ID;
use async_graphql::SimpleObject;

#[derive(SimpleObject)]
#[graphql(complex)]
pub struct ChatItem {
    pub id: ID,
    pub from_id: ID,
    pub to_id: ID,
    pub channel_id: Option<ID>,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
    pub content: String,
    pub status: ChatStatus,
    pub status_data: String,
}

impl From<DChat> for ChatItem {
    fn from(c: DChat) -> Self {
        Self {
            id: c.id.into(),
            from_id: c.from_id.into(),
            to_id: c.to_id.into(),
            channel_id: if c.channel_id.is_empty() {
                None
            } else {
                Some(c.channel_id.into())
            },
            created_at: c.created_at,
            updated_at: c.updated_at,
            content: c.content,
            status: c.status.into(),
            status_data: c.status_data,
        }
    }
}

#[derive(SimpleObject, Clone)]
pub struct ChatChannelMember {
    pub peer_id: ID,
    pub status: MemberStatus,
}

#[derive(SimpleObject)]
#[graphql(complex)]
pub struct ChatChannel {
    pub id: ID,
    pub name: String,
    pub owner_id: ID,
    pub members: Vec<ChatChannelMember>,
    pub version: Long,
    pub status: ChannelStatus,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

impl From<DChannel> for ChatChannel {
    fn from(ch: DChannel) -> Self {
        let members = plain_rs::chat::channels::messages::decode_members(&ch.members)
            .into_iter()
            .map(|m| ChatChannelMember {
                peer_id: m.peer_id.into(),
                status: m.status.into(),
            })
            .collect();
        Self {
            id: ch.id.into(),
            name: ch.name,
            owner_id: ch.owner_id.into(),
            members,
            version: Long(ch.version),
            status: ch.status.into(),
            created_at: ch.created_at,
            updated_at: ch.updated_at,
        }
    }
}

#[async_graphql::ComplexObject]
impl ChatItem {
    async fn created_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.created_at)
    }
    async fn updated_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.updated_at)
    }
}

#[async_graphql::ComplexObject]
impl ChatChannel {
    async fn created_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.created_at)
    }
    async fn updated_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.updated_at)
    }
}
