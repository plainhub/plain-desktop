//! Public `/graphql` contact roots.
//!
//! The search DSL stays owned by Rust: the host turns the query into the
//! same `WHERE` clause the app's own contact provider view uses
//! ([`super::provider_plan`]) and runs it against ContactsContract, so
//! matching and paging stay the platform's job exactly as they are today.
//!
//! Writes go through [`super::contact_write`], which already validates the
//! payload and enforces the `WRITE_CONTACTS` permission.

use super::contact_write;
use super::contact_write::Action;
use super::public_contact_types::*;
use super::public_facts::{host_json, instant, integer, list, text};
use super::public_gate;
use crate::content_api::host::Host;
use crate::content_types::ActionResult;
use crate::{db::Db, enums::DataType, prefs::Prefs};
use async_graphql::{Context, ID, Object};
use serde_json::{Value, json};
use std::sync::Arc;

const READ: &str = "READ_CONTACTS";
const WRITE: &str = "WRITE_CONTACTS";

#[derive(Default)]
pub struct ContactsQuery;

#[Object]
impl ContactsQuery {
    async fn contacts(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<Contact>> {
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[READ])?;
        let facts = host_json(
            ctx,
            "systemContactFacts",
            json!({"query": query, "offset": offset, "limit": limit}),
        )
        .await?;
        let db = ctx.data_unchecked::<Arc<Db>>();
        Ok(facts.iter().map(|fact| contact(db, fact)).collect())
    }

    /// plain-app degrades this to 0 rather than erroring when the web client
    /// was not granted the contact permission.
    async fn contact_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[WRITE]).await? {
            return Ok(0);
        }
        let count = ctx
            .data_unchecked::<Arc<Host>>()
            .call("systemContactCount", json!({ "query": query }))
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        Ok(count.as_i64().unwrap_or_default() as i32)
    }

    async fn contact_sources(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<ContactSource>> {
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[READ])?;
        Ok(host_json(ctx, "systemContactSources", json!({}))
            .await?
            .iter()
            .map(|fact| ContactSource {
                name: text(fact, "name"),
                r#type: text(fact, "type"),
            })
            .collect())
    }

    async fn contact_groups(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<ContactGroup>> {
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[READ])?;
        Ok(host_json(ctx, "systemContactGroupFacts", json!({}))
            .await?
            .iter()
            .map(group)
            .collect())
    }
}

#[derive(Default)]
pub struct ContactsMutation;

#[Object]
impl ContactsMutation {
    async fn create_contact(
        &self,
        ctx: &Context<'_>,
        input: ContactInput,
    ) -> async_graphql::Result<Contact> {
        let id = contact_write_call(
            ctx,
            contact_write::Request {
                action: Action::CreateContact,
                id: None,
                input: Some(contact_input(&input)),
                name: None,
                account_name: None,
                account_type: None,
            },
        )
        .await?;
        reload(ctx, &id, "after create").await
    }

    async fn update_contact(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: ContactInput,
    ) -> async_graphql::Result<Contact> {
        contact_write_call(
            ctx,
            contact_write::Request {
                action: Action::UpdateContact,
                id: Some(id.0.clone()),
                input: Some(contact_input(&input)),
                name: None,
                account_name: None,
                account_type: None,
            },
        )
        .await?;
        reload(ctx, &id.0, "after update").await
    }

    async fn delete_contacts(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        if query.trim().is_empty() {
            return Err(async_graphql::Error::new(
                "query is required for bulk mutations — pass 'all:true' to explicitly target everything (API_SPEC §5)",
            ));
        }
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[WRITE])?;
        let ids: Vec<Value> = host_json(ctx, "systemContactIds", json!({ "query": query }))
            .await?
            .iter()
            .filter_map(Value::as_str)
            .map(Value::from)
            .collect();
        let deleted: Vec<Value> = serde_json::from_value(
            ctx.data_unchecked::<Arc<Host>>()
                .call(
                    "systemDeleteRecords",
                    json!({"provider": "CONTACT", "ids": ids}),
                )
                .await
                .map_err(|error| async_graphql::Error::new(error))?,
        )
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(ActionResult {
            affected_count: deleted.len() as i32,
        })
    }

    async fn create_contact_group(
        &self,
        ctx: &Context<'_>,
        name: String,
        account_name: String,
        account_type: String,
    ) -> async_graphql::Result<ContactGroup> {
        let created = contact_write_call(
            ctx,
            contact_write::Request {
                action: Action::CreateGroup,
                id: None,
                input: None,
                name: Some(name.clone()),
                account_name: Some(account_name),
                account_type: Some(account_type),
            },
        )
        .await?;
        Ok(ContactGroup {
            id: ID::from(created),
            name,
            contact_count: 0,
        })
    }

    async fn update_contact_group(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
    ) -> async_graphql::Result<ContactGroup> {
        contact_write_call(
            ctx,
            contact_write::Request {
                action: Action::UpdateGroup,
                id: Some(id.0.clone()),
                input: None,
                name: Some(name.clone()),
                account_name: None,
                account_type: None,
            },
        )
        .await?;
        Ok(ContactGroup {
            id,
            name,
            contact_count: 0,
        })
    }

    async fn delete_contact_group(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<bool> {
        contact_write_call(
            ctx,
            contact_write::Request {
                action: Action::DeleteGroup,
                id: Some(id.0),
                input: None,
                name: None,
                account_name: None,
                account_type: None,
            },
        )
        .await?;
        Ok(true)
    }
}

/// Returns the id the platform assigned to a newly created row; every other
/// action comes back without one.
async fn contact_write_call(
    ctx: &Context<'_>,
    request: contact_write::Request,
) -> async_graphql::Result<String> {
    let result = contact_write::execute(
        ctx.data_unchecked::<Arc<Prefs>>(),
        ctx.data_unchecked::<Arc<Host>>(),
        request,
    )
    .await
    .map_err(|error| async_graphql::Error::new(error.to_string()))?;
    Ok(result["id"].as_str().unwrap_or_default().to_owned())
}

/// The contract returns the freshly written contact, so the row is read back
/// through the same provider search instead of being assembled from the input.
async fn reload(ctx: &Context<'_>, id: &str, phase: &str) -> async_graphql::Result<Contact> {
    let facts = host_json(
        ctx,
        "systemContactFacts",
        json!({"query": format!("id={id}"), "offset": 0, "limit": 1}),
    )
    .await?;
    let fact = facts
        .iter()
        .find(|fact| text(fact, "id") == id)
        .ok_or_else(|| async_graphql::Error::new(format!("Contact {id} not found {phase}")))?;
    Ok(contact(ctx.data_unchecked::<Arc<Db>>(), fact))
}

fn contact(db: &Arc<Db>, fact: &Value) -> Contact {
    let id = text(fact, "id");
    Contact {
        tags: crate::db::tag::tags_for_key_of_kind(db, &id, DataType::Contact.kind())
            .unwrap_or_default()
            .into_iter()
            .map(|tag| Tag {
                id: ID::from(tag.id),
                name: tag.name,
                count: tag.count,
            })
            .collect(),
        id: ID::from(id),
        prefix: text(fact, "prefix"),
        first_name: text(fact, "firstName"),
        middle_name: text(fact, "middleName"),
        last_name: text(fact, "lastName"),
        suffix: text(fact, "suffix"),
        nickname: text(fact, "nickname"),
        photo_id: text(fact, "photoId"),
        phone_numbers: list(fact, "phoneNumbers", |item| ContactPhoneNumber {
            value: text(item, "value"),
            r#type: PhoneType::from_android(integer(item, "type")),
            label: text(item, "label"),
            normalized_number: text(item, "normalizedNumber"),
        }),
        emails: list(fact, "emails", |item| ContactEmail {
            value: text(item, "value"),
            r#type: EmailType::from_android(integer(item, "type")),
            label: text(item, "label"),
        }),
        addresses: list(fact, "addresses", |item| ContactAddress {
            value: text(item, "value"),
            r#type: PostalType::from_android(integer(item, "type")),
            label: text(item, "label"),
        }),
        events: list(fact, "events", |item| ContactEvent {
            value: text(item, "value"),
            r#type: EventType::from_android(integer(item, "type")),
            label: text(item, "label"),
        }),
        websites: list(fact, "websites", |item| ContactWebsite {
            value: text(item, "value"),
            r#type: WebsiteType::from_android(integer(item, "type")),
            label: text(item, "label"),
        }),
        ims: list(fact, "ims", |item| ContactIm {
            value: text(item, "value"),
            protocol: ImProtocol::from_android(integer(item, "type")),
            custom_protocol: text(item, "label"),
        }),
        source: text(fact, "source"),
        starred: fact["starred"].as_bool().unwrap_or(false),
        contact_id: ID::from(text(fact, "contactId")),
        thumbnail_id: text(fact, "thumbnailId"),
        notes: text(fact, "notes"),
        groups: list(fact, "groups", group),
        organization: organization(fact),
        ringtone: text(fact, "ringtone"),
        updated_at: instant(fact, "updatedAt"),
    }
}

fn organization(fact: &Value) -> Option<Organization> {
    let org = fact["organization"].as_object()?;
    Some(Organization {
        company: text(&Value::Object(org.clone()), "company"),
        title: text(&Value::Object(org.clone()), "title"),
    })
}

fn group(fact: &Value) -> ContactGroup {
    ContactGroup {
        id: ID::from(text(fact, "id")),
        name: text(fact, "name"),
        contact_count: 0,
    }
}

/// The payload shape the platform's `ContactInput` deserializes.
fn contact_input(input: &ContactInput) -> Value {
    json!({
        "prefix": input.prefix,
        "firstName": input.first_name,
        "middleName": input.middle_name,
        "lastName": input.last_name,
        "suffix": input.suffix,
        "nickname": input.nickname,
        "phoneNumbers": input.phone_numbers.iter().map(|item| json!({
            "value": item.value, "type": item.r#type.name(), "label": item.label,
        })).collect::<Vec<_>>(),
        "emails": input.emails.iter().map(|item| json!({
            "value": item.value, "type": item.r#type.name(), "label": item.label,
        })).collect::<Vec<_>>(),
        "addresses": input.addresses.iter().map(|item| json!({
            "value": item.value, "type": item.r#type.name(), "label": item.label,
        })).collect::<Vec<_>>(),
        "events": input.events.iter().map(|item| json!({
            "value": item.value, "type": item.r#type.name(), "label": item.label,
        })).collect::<Vec<_>>(),
        "websites": input.websites.iter().map(|item| json!({
            "value": item.value, "type": item.r#type.name(), "label": item.label,
        })).collect::<Vec<_>>(),
        "ims": input.ims.iter().map(|item| json!({
            "value": item.value, "protocol": item.protocol.name(), "customProtocol": item.custom_protocol,
        })).collect::<Vec<_>>(),
        "source": input.source,
        "starred": input.starred,
        "notes": input.notes,
        "groupIds": input.group_ids.iter().map(|id| Value::from(id.0.clone())).collect::<Vec<_>>(),
        "organization": input.organization.as_ref().map(|org| json!({
            "company": org.company, "title": org.title,
        })),
    })
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_contacts.rs"]
mod tests;
