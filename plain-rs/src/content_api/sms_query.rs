use super::{
    provider_plan::{self, Plan},
    server::ServerState,
};
use crate::{db::Db, utils::search_dsl::FilterField};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Search {
        query: String,
        offset: i32,
        limit: i32,
        #[serde(rename = "includeTrashed")]
        include_trashed: bool,
    },
    Count {
        query: String,
    },
    Ids {
        query: String,
        #[serde(rename = "includeTrashed")]
        include_trashed: bool,
    },
    Counts,
    Conversations {
        query: String,
        offset: i32,
        limit: i32,
    },
    ConversationCount {
        query: String,
    },
    ArchivedConversations {
        query: String,
        offset: i32,
        limit: i32,
    },
    ConversationDate {
        id: String,
    },
    TextIds {
        filters: Vec<String>,
    },
}
#[derive(Serialize)]
struct Plans {
    sms: Plan,
    mms: Option<Plan>,
    #[serde(rename = "threadId")]
    thread_id: String,
}
fn numeric_ids(plan: &mut Plan, column: &str, ids: Vec<String>, exclude: bool) -> bool {
    if ids.is_empty()
        || ids
            .iter()
            .any(|id| id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    let operator = if exclude { "NOT IN" } else { "IN" };
    let mut clauses = Vec::new();
    for chunk in ids.chunks(500) {
        clauses.push(format!(
            "{column} {operator} ({})",
            vec!["?"; chunk.len()].join(",")
        ));
    }
    plan.add(
        format!("({})", clauses.join(if exclude { " AND " } else { " OR " })),
        ids,
    );
    true
}
fn plans(
    db: &Db,
    fields: &[FilterField],
    include_trashed: bool,
    text_ids: Option<Vec<String>>,
) -> anyhow::Result<Plans> {
    let mut sms = Plan::default();
    let mut mms = Some(Plan::default());
    for f in fields {
        match f.name.as_str() {
            "text" => sms.contains("body", &f.value),
            "ids" => {
                let ids = f
                    .value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                let sms_ids = ids
                    .iter()
                    .filter(|id| !id.starts_with("mms_"))
                    .map(|id| (*id).to_owned())
                    .collect();
                if !numeric_ids(&mut sms, "_id", sms_ids, false) {
                    sms.equal("_id", "-1");
                }
                if let Some(plan) = mms.as_mut() {
                    let mms_ids = ids
                        .iter()
                        .filter_map(|id| id.strip_prefix("mms_"))
                        .map(str::to_owned)
                        .collect();
                    if !numeric_ids(plan, "_id", mms_ids, false) {
                        mms = None;
                    }
                }
            }
            "type" => {
                sms.equal("type", &f.value);
                if let Some(plan) = mms.as_mut() {
                    plan.equal("msg_box", &f.value);
                }
            }
            "thread_id" => {
                sms.equal("thread_id", &f.value);
                if let Some(plan) = mms.as_mut() {
                    plan.equal("thread_id", &f.value);
                }
            }
            _ => {}
        }
    }
    if let Some(ids) = text_ids {
        if let Some(plan) = mms.as_mut() {
            if !numeric_ids(plan, "_id", ids, false) {
                mms = None;
            }
        }
    }
    if let Some(plan) = mms.as_mut() {
        plan.add("m_type IN (128,132)".into(), vec![]);
    }
    let trashed = if include_trashed {
        vec![]
    } else {
        db.trashed_message_ids()?
    };
    let show_trashed = fields.iter().any(|f| f.name == "trashed" && f.value == "1");
    let sms_ids = trashed
        .iter()
        .filter(|id| !id.starts_with("mms_"))
        .cloned()
        .collect();
    let mms_ids = trashed
        .iter()
        .filter_map(|id| id.strip_prefix("mms_"))
        .map(str::to_owned)
        .collect();
    let sms_filtered = numeric_ids(&mut sms, "_id", sms_ids, !show_trashed);
    if show_trashed && !sms_filtered {
        sms.equal("_id", "-1");
    }
    if let Some(plan) = mms.as_mut() {
        let filtered = numeric_ids(plan, "_id", mms_ids, !show_trashed);
        if show_trashed && !filtered {
            plan.equal("_id", "-1");
        }
    }
    let thread = fields
        .iter()
        .find(|f| f.name == "thread_id")
        .map(|f| f.value.clone())
        .unwrap_or_default();
    if let Some(archive) = db
        .archived_conversation_list()?
        .into_iter()
        .find(|a| a.conversation_id == thread)
    {
        let millis =
            chrono::DateTime::parse_from_rfc3339(&archive.conversation_date)?.timestamp_millis();
        let op = if fields
            .iter()
            .any(|f| f.name == "archived" && f.value == "1")
        {
            "<="
        } else {
            ">"
        };
        sms.add(format!("date {op} ?"), vec![millis.to_string()]);
        if let Some(plan) = mms.as_mut() {
            plan.add(format!("date {op} ?"), vec![(millis / 1000).to_string()]);
        }
    }
    Ok(Plans {
        sms,
        mms,
        thread_id: thread,
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadFacts {
    id: String,
    snippet: String,
    date: String,
    message_count: i32,
    read: bool,
    #[serde(default)]
    addresses: Vec<String>,
}
fn instant(value: &str) -> anyhow::Result<chrono::DateTime<chrono::FixedOffset>> {
    Ok(chrono::DateTime::parse_from_rfc3339(value)?)
}
fn addresses_match(first: &str, second: &str, candidates: &[String]) -> bool {
    let normalize = |value: &str| {
        let trimmed = value.trim();
        let phone = !trimmed.is_empty()
            && trimmed.chars().any(char::is_numeric)
            && trimmed
                .chars()
                .all(|c| c.is_numeric() || c.is_whitespace() || "+-()./".contains(c));
        if phone {
            (
                trimmed
                    .chars()
                    .filter(|c| c.is_numeric())
                    .collect::<String>(),
                true,
            )
        } else {
            (trimmed.to_lowercase(), false)
        }
    };
    let (a, ap) = normalize(first);
    let (b, bp) = normalize(second);
    if a == b {
        return true;
    }
    if !ap || !bp || a.len().min(b.len()) < 7 || a.len() == b.len() {
        return false;
    }
    let suffix = |longer: &str, shorter: &str| longer.ends_with(shorter);
    let matches = candidates
        .iter()
        .map(|candidate| normalize(candidate))
        .filter(|(v, is_phone)| {
            *is_phone
                && ((v.len() > b.len() && suffix(v, &b)) || (v.len() > a.len() && suffix(v, &a)))
        })
        .collect::<std::collections::HashSet<_>>();
    matches.len() == 1 && matches.contains(&(a, true))
}
fn addresses(mut candidates: Vec<String>, own: &[String]) -> Vec<String> {
    candidates.retain(|v| !v.trim().is_empty());
    let mut distinct: Vec<String> = Vec::new();
    for candidate in candidates.clone() {
        if !distinct
            .iter()
            .any(|v| addresses_match(v, &candidate, &candidates))
        {
            distinct.push(candidate)
        }
    }
    let mut non_self = distinct
        .iter()
        .filter(|address| {
            !own.iter()
                .any(|own| addresses_match(address, own, &distinct))
        })
        .cloned()
        .collect::<Vec<_>>();
    if non_self.is_empty() {
        non_self = distinct;
    }
    non_self
}
fn archived_matches(addresses: &[String], snippet: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || addresses
            .iter()
            .any(|address| address.to_lowercase().contains(&query))
        || snippet.to_lowercase().contains(&query)
}
fn conversation_models(
    db: &Db,
    rows: Vec<ThreadFacts>,
    query: &str,
    offset: i32,
    limit: i32,
) -> anyhow::Result<Vec<Value>> {
    let active = db
        .archived_conversation_list()?
        .into_iter()
        .filter_map(|archive| {
            rows.iter()
                .find(|row| row.id == archive.conversation_id)
                .and_then(
                    |row| match (instant(&row.date), instant(&archive.conversation_date)) {
                        (Ok(date), Ok(saved)) if date <= saved => Some(archive.conversation_id),
                        _ => None,
                    },
                )
        })
        .collect::<std::collections::HashSet<_>>();
    let mut rows = rows
        .into_iter()
        .filter(|row| !active.contains(&row.id))
        .collect::<Vec<_>>();
    if !query.is_empty() {
        let fields = provider_plan::fields(db, query)?;
        rows.retain(|row| {
            fields.iter().all(|f| match f.name.as_str() {
                "ids" => f.value.split(',').any(|v| v == row.id),
                "thread_id" => row.id == f.value,
                "type" | "trashed" | "archived" | "all" => true,
                _ => true,
            })
        });
    }
    rows.sort_by(|a, b| instant(&b.date).ok().cmp(&instant(&a.date).ok()));
    Ok(rows.into_iter().skip(offset.max(0) as usize).take(limit.max(0) as usize).map(|row|{
        let selected=addresses(row.addresses,&[]);
        json!({"id":row.id,"address":selected.first().cloned().unwrap_or_default(),"addresses":selected,"snippet":row.snippet,"date":row.date,"messageCount":row.message_count,"read":row.read})
    }).collect())
}
fn text_ids(facts: Value, filters: &[String]) -> anyhow::Result<Vec<String>> {
    let parts: std::collections::BTreeMap<String, Vec<String>> = serde_json::from_value(facts)?;
    Ok(parts
        .into_iter()
        .filter(|(_, parts)| {
            let body = parts.join("\n").to_lowercase();
            filters.iter().all(|q| body.contains(&q.to_lowercase()))
        })
        .map(|(id, _)| id)
        .collect())
}
async fn prepare(
    db: &crate::db::Db,
    host: &super::host::Host,
    query: &str,
    include_trashed: bool,
) -> anyhow::Result<Plans> {
    let fields = provider_plan::fields(&db, query)?;
    let filters = fields
        .iter()
        .filter(|f| f.name == "text")
        .map(|f| f.value.clone())
        .collect::<Vec<_>>();
    let ids = if filters.is_empty() {
        None
    } else {
        Some(text_ids(
            host.call("systemMmsTextFacts", json!({}))
                .await
                .map_err(anyhow::Error::msg)?,
            &filters,
        )?)
    };
    plans(&db, &fields, include_trashed, ids)
}
async fn count(
    db: &crate::db::Db,
    host: &super::host::Host,
    query: &str,
    include_trashed: bool,
) -> anyhow::Result<i64> {
    let plans = prepare(db, host, query, include_trashed).await?;
    let receipt = host
        .call("systemSmsCountFacts", serde_json::to_value(plans)?)
        .await
        .map_err(anyhow::Error::msg)?;
    let sms = receipt["sms"]
        .as_i64()
        .filter(|v| *v >= 0)
        .ok_or_else(|| anyhow::anyhow!("invalid SMS count"))?;
    let mms = receipt["mms"]
        .as_i64()
        .filter(|v| *v >= 0)
        .ok_or_else(|| anyhow::anyhow!("invalid MMS count"))?;
    sms.checked_add(mms)
        .ok_or_else(|| anyhow::anyhow!("SMS count overflow"))
}
fn page(
    mut items: Vec<Value>,
    offset: i32,
    limit: i32,
    canonical: &str,
) -> anyhow::Result<Vec<Value>> {
    for item in &mut items {
        anyhow::ensure!(
            item["date"]
                .as_str()
                .is_some_and(|t| chrono::DateTime::parse_from_rfc3339(t).is_ok()),
            "invalid SMS date"
        );
        if item["address"].as_str() == Some("") && !canonical.is_empty() {
            item["address"] = json!(canonical);
        }
    }
    items.sort_by(|a, b| {
        let date =
            |v: &Value| chrono::DateTime::parse_from_rfc3339(v["date"].as_str().unwrap()).unwrap();
        date(b).cmp(&date(a))
    });
    Ok(items
        .into_iter()
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect())
}
async fn matched_conversation_facts(
    db: &crate::db::Db,
    host: &super::host::Host,
    query: &str,
) -> anyhow::Result<Vec<ThreadFacts>> {
    let mut plans = prepare(db, host, query, true).await?;
    plans.sms.clauses.retain(|c| !c.starts_with("date "));
    plans
        .mms
        .as_mut()
        .into_iter()
        .for_each(|m| m.clauses.retain(|c| !c.starts_with("date ")));
    let hits: Vec<(String, String)> = serde_json::from_value(
        host.call("systemSmsThreadFacts", json!({"plans":plans}))
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    let mut latest = std::collections::HashMap::<String, String>::new();
    for (id, date) in hits {
        if id.is_empty() {
            continue;
        }
        if latest
            .get(&id)
            .is_none_or(|old| instant(&date).ok() > instant(old).ok())
        {
            latest.insert(id, date);
        }
    }
    let mut latest = latest.into_iter().collect::<Vec<_>>();
    latest.sort_by(|a, b| instant(&b.1).ok().cmp(&instant(&a.1).ok()));
    let ids = latest.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
    conversation_facts(host, Some(ids), None).await
}
async fn conversation_facts(
    host: &super::host::Host,
    ids: Option<Vec<String>>,
    limit: Option<i64>,
) -> anyhow::Result<Vec<ThreadFacts>> {
    let facts = host
        .call(
            "systemSmsConversationFacts",
            json!({"ids":ids,"limit":limit}),
        )
        .await
        .map_err(anyhow::Error::msg)?;
    Ok(serde_json::from_value(facts["items"].clone())?)
}
pub(super) async fn execute(
    db: &crate::db::Db,
    host: &super::host::Host,
    request: Request,
) -> anyhow::Result<Value> {
    Ok(match request {
        Request::Search {
            query,
            offset,
            limit,
            include_trashed,
        } => {
            if limit <= 0 {
                return Ok(json!({"items":[]}));
            }
            let plans = prepare(db, host, &query, include_trashed).await?;
            let receipt = host
                .call(
                    "systemSmsRowsFacts",
                    json!({"plans":plans,"limit":i64::from(offset.max(0))+i64::from(limit)}),
                )
                .await
                .map_err(anyhow::Error::msg)?;
            let canonical = receipt["canonicalAddress"].as_str().unwrap_or_default();
            json!({"items":page(serde_json::from_value(receipt["items"].clone())?,offset,limit,canonical)?})
        }
        Request::Count { query } => json!({"count":count(db,host,&query,false).await?}),
        Request::Ids {
            query,
            include_trashed,
        } => {
            let plans = prepare(db, host, &query, include_trashed).await?;
            let ids: Vec<String> = serde_json::from_value(
                host.call("systemSmsIdsFacts", serde_json::to_value(plans)?)
                    .await
                    .map_err(anyhow::Error::msg)?,
            )?;
            json!({"ids":ids.into_iter().collect::<std::collections::BTreeSet<_>>()})
        }
        Request::Counts => {
            json!({"total":count(db,host,"",true).await?,"inbox":count(db,host,"type:1",true).await?,"sent":count(db,host,"type:2",true).await?,"drafts":count(db,host,"type:3",true).await?})
        }
        Request::Conversations {
            query,
            offset,
            limit,
        } => {
            let rows = if query.is_empty() {
                conversation_facts(
                    host,
                    None,
                    Some(
                        i64::from(offset.max(0))
                            + i64::from(limit.max(0))
                            + i64::from(db.archived_conversation_list()?.len() as i32),
                    ),
                )
                .await?
            } else {
                matched_conversation_facts(db, host, &query).await?
            };
            json!({"items":conversation_models(&db,rows,&query,offset,limit)?})
        }
        Request::ConversationCount { query } => {
            let rows = if query.is_empty() {
                conversation_facts(host, None, None).await?
            } else {
                matched_conversation_facts(db, host, &query).await?
            };
            json!({"count":conversation_models(&db,rows,&query,0,i32::MAX)?.len()})
        }
        Request::ArchivedConversations {
            query,
            offset,
            limit,
        } => {
            let records = db.archived_conversation_list()?;
            let ids = records
                .iter()
                .map(|r| r.conversation_id.clone())
                .collect::<Vec<_>>();
            let before = records
                .iter()
                .map(|r| {
                    Ok((
                        r.conversation_id.clone(),
                        instant(&r.conversation_date)?.timestamp_millis(),
                    ))
                })
                .collect::<anyhow::Result<std::collections::HashMap<_, _>>>()?;
            let facts = host
                .call(
                    "systemSmsConversationFacts",
                    json!({"ids":ids,"beforeDates":before}),
                )
                .await
                .map_err(anyhow::Error::msg)?;
            let rows: Vec<ThreadFacts> = serde_json::from_value(facts["items"].clone())?;
            let by_id = rows
                .into_iter()
                .map(|r| (r.id.clone(), r))
                .collect::<std::collections::HashMap<_, _>>();
            let items=records.into_iter().filter_map(|record|by_id.get(&record.conversation_id).map(|row|{
                let date=instant(&record.conversation_date).ok()?;
                let snippet=if let Some(value)=facts["snippets"][&row.id].as_str(){value.to_owned()}else{row.snippet.clone()};
                let addresses = addresses(row.addresses.clone(),&[]);
                if !archived_matches(&addresses, &snippet, &query) {
                    return None;
                }
                Some(json!({"id":row.id,"address":addresses.first().cloned().unwrap_or_default(),"addresses":addresses,"snippet":snippet,"date":date.to_rfc3339(),"messageCount":row.message_count,"read":row.read}))
            }).flatten()).collect::<Vec<_>>();
            json!({"items":items.into_iter().skip(offset.max(0) as usize).take(limit.max(0) as usize).collect::<Vec<_>>()})
        }
        Request::ConversationDate { id } => {
            let rows = conversation_facts(host, Some(vec![id]), None).await?;
            json!({"date":rows.first().map(|r|&r.date)})
        }
        Request::TextIds { filters } => {
            json!({"ids":text_ids(host.call("systemMmsTextFacts",json!({})).await.map_err(anyhow::Error::msg)?,&filters)?})
        }
    })
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state.db, &state.host, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/sms_query.rs"]
mod tests;
