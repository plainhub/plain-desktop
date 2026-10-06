//! Contract model for the public contact roots.
//!
//! Each detail enum mirrors one ContactsContract CommonDataKinds row: its
//! member order is the Android `DATA2` code, and any code the platform does
//! not recognise reads back as `CUSTOM` — the label (or `customProtocol`)
//! string then carries the user-visible value, matching Android's own
//! convention.

use crate::content_types::Instant;
use async_graphql::{Enum, ID, InputObject, SimpleObject};

/// The GraphQL enum name, which is also what the platform's kotlinx enum
/// deserializer expects. Derived from the variant with the same case-boundary
/// rule async-graphql uses, so the two cannot drift apart — the schema test
/// pins that agreement against the rendered SDL.
fn screaming_snake(variant: &str) -> String {
    let mut name = String::new();
    for (index, ch) in variant.char_indices() {
        if ch.is_uppercase() && index > 0 && !variant[..index].ends_with(char::is_uppercase) {
            name.push('_');
        }
        name.extend(ch.to_uppercase());
    }
    name
}

macro_rules! data_kind {
    ($name:ident { $($variant:ident = $code:literal),* $(,)? }) => {
        #[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
        pub enum $name {
            #[default]
            Custom,
            $($variant,)*
        }
        impl $name {
            pub(crate) fn from_android(value: i64) -> Self {
                match value {
                    $($code => Self::$variant,)*
                    _ => Self::Custom,
                }
            }
            pub(crate) fn android(self) -> i64 {
                match self {
                    Self::Custom => 0,
                    $(Self::$variant => $code,)*
                }
            }
            pub(crate) fn name(self) -> String {
                match self {
                    Self::Custom => "CUSTOM".to_owned(),
                    $(Self::$variant => screaming_snake(stringify!($variant)),)*
                }
            }
        }
    };
}

data_kind!(PhoneType {
    Home = 1,
    Mobile = 2,
    Work = 3,
    FaxWork = 4,
    FaxHome = 5,
    Pager = 6,
    Other = 7,
    Callback = 8,
    Car = 9,
    CompanyMain = 10,
    Isdn = 11,
    Main = 12,
    OtherFax = 13,
    Radio = 14,
    Telex = 15,
    TtyDdd = 16,
    WorkMobile = 17,
    WorkPager = 18,
    Assistant = 19,
});

data_kind!(EmailType {
    Home = 1,
    Work = 2,
    Other = 3,
    Mobile = 4,
});

data_kind!(PostalType {
    Home = 1,
    Work = 2,
    Other = 3,
});

data_kind!(EventType {
    Anniversary = 1,
    Birthday = 2,
    Other = 3,
});

data_kind!(WebsiteType {
    Homepage = 1,
    Blog = 2,
    Ftp = 3,
    Home = 4,
    Work = 5,
    Other = 6,
});

data_kind!(ImProtocol {
    Aim = 1,
    Msn = 2,
    Yahoo = 3,
    Skype = 4,
    Qq = 5,
    GoogleTalk = 6,
    Icq = 7,
    Jabber = 8,
    Netmeeting = 9,
});

#[derive(SimpleObject, Clone, Debug)]
pub struct ContactPhoneNumber {
    pub value: String,
    pub r#type: PhoneType,
    pub label: String,
    #[graphql(name = "normalizedNumber")]
    pub normalized_number: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ContactEmail {
    pub value: String,
    pub r#type: EmailType,
    pub label: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ContactAddress {
    pub value: String,
    pub r#type: PostalType,
    pub label: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ContactEvent {
    pub value: String,
    pub r#type: EventType,
    pub label: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ContactWebsite {
    pub value: String,
    pub r#type: WebsiteType,
    pub label: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ContactIm {
    pub value: String,
    pub protocol: ImProtocol,
    #[graphql(name = "customProtocol")]
    pub custom_protocol: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Organization {
    pub company: String,
    pub title: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ContactGroup {
    pub id: ID,
    pub name: String,
    #[graphql(name = "contactCount")]
    pub contact_count: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ContactSource {
    pub name: String,
    pub r#type: String,
}

/// The contract's `Tag` carries no numeric kind — that lives in the
/// `tags(type:)` filter instead — so it is not `content_types::Tag`.
#[derive(SimpleObject, Clone, Debug)]
pub struct Tag {
    pub id: ID,
    pub name: String,
    pub count: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Contact {
    pub id: ID,
    pub prefix: String,
    #[graphql(name = "firstName")]
    pub first_name: String,
    #[graphql(name = "middleName")]
    pub middle_name: String,
    #[graphql(name = "lastName")]
    pub last_name: String,
    pub suffix: String,
    pub nickname: String,
    #[graphql(name = "photoId")]
    pub photo_id: String,
    #[graphql(name = "phoneNumbers")]
    pub phone_numbers: Vec<ContactPhoneNumber>,
    pub emails: Vec<ContactEmail>,
    pub addresses: Vec<ContactAddress>,
    pub events: Vec<ContactEvent>,
    pub source: String,
    pub starred: bool,
    #[graphql(name = "contactId")]
    pub contact_id: ID,
    #[graphql(name = "thumbnailId")]
    pub thumbnail_id: String,
    pub notes: String,
    pub groups: Vec<ContactGroup>,
    pub organization: Option<Organization>,
    pub websites: Vec<ContactWebsite>,
    pub ims: Vec<ContactIm>,
    pub ringtone: String,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub tags: Vec<Tag>,
}

#[derive(InputObject, Clone, Debug)]
pub struct ContactPhoneInput {
    pub value: String,
    pub r#type: PhoneType,
    pub label: String,
}

#[derive(InputObject, Clone, Debug)]
pub struct ContactEmailInput {
    pub value: String,
    pub r#type: EmailType,
    pub label: String,
}

#[derive(InputObject, Clone, Debug)]
pub struct ContactAddressInput {
    pub value: String,
    pub r#type: PostalType,
    pub label: String,
}

#[derive(InputObject, Clone, Debug)]
pub struct ContactEventInput {
    pub value: String,
    pub r#type: EventType,
    pub label: String,
}

#[derive(InputObject, Clone, Debug)]
pub struct ContactWebsiteInput {
    pub value: String,
    pub r#type: WebsiteType,
    pub label: String,
}

#[derive(InputObject, Clone, Debug)]
pub struct ContactImInput {
    pub value: String,
    pub protocol: ImProtocol,
    #[graphql(name = "customProtocol")]
    pub custom_protocol: String,
}

#[derive(InputObject, Clone, Debug)]
pub struct OrganizationInput {
    pub company: String,
    pub title: String,
}

#[derive(InputObject, Clone, Debug)]
pub struct ContactInput {
    pub prefix: String,
    #[graphql(name = "firstName")]
    pub first_name: String,
    #[graphql(name = "middleName")]
    pub middle_name: String,
    #[graphql(name = "lastName")]
    pub last_name: String,
    pub suffix: String,
    pub nickname: String,
    #[graphql(name = "phoneNumbers")]
    pub phone_numbers: Vec<ContactPhoneInput>,
    pub emails: Vec<ContactEmailInput>,
    pub addresses: Vec<ContactAddressInput>,
    pub events: Vec<ContactEventInput>,
    pub source: String,
    pub starred: bool,
    pub notes: String,
    #[graphql(name = "groupIds")]
    pub group_ids: Vec<ID>,
    pub organization: Option<OrganizationInput>,
    pub websites: Vec<ContactWebsiteInput>,
    pub ims: Vec<ContactImInput>,
}
