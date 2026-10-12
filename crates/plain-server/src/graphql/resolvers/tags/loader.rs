use super::domain_types::Tag;
use async_graphql::{
    Context,
    dataloader::{DataLoader, Loader},
};
use plain_rs::{db::Db, enums::DataType};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct TagKey {
    kind: i32,
    id: String,
}

pub(crate) struct TagsLoader {
    db: Arc<Db>,
    #[cfg(test)]
    pub(crate) loads: std::sync::atomic::AtomicUsize,
}

pub(crate) fn new(db: Arc<Db>) -> DataLoader<TagsLoader> {
    DataLoader::new(
        TagsLoader {
            db,
            #[cfg(test)]
            loads: Default::default(),
        },
        tokio::spawn,
    )
}

impl Loader<TagKey> for TagsLoader {
    type Value = Vec<Tag>;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[TagKey]) -> Result<HashMap<TagKey, Vec<Tag>>, Self::Error> {
        #[cfg(test)]
        self.loads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut groups: HashMap<i32, Vec<String>> = HashMap::new();
        for key in keys {
            groups.entry(key.kind).or_default().push(key.id.clone());
        }
        let mut result: HashMap<_, _> = keys.iter().cloned().map(|key| (key, Vec::new())).collect();
        for (kind, ids) in groups {
            let tags: HashMap<_, _> = plain_rs::tags::tags_by_type(&self.db, kind)?
                .into_iter()
                .map(|tag| (tag.id.clone(), tag))
                .collect();
            for relation in plain_rs::tags::relations_for_keys_of_kind(&self.db, &ids, kind)? {
                if let Some(tag) = tags.get(&relation.tag_id) {
                    result
                        .entry(TagKey {
                            kind,
                            id: relation.key,
                        })
                        .or_default()
                        .push(Tag {
                            id: tag.id.clone().into(),
                            name: tag.name.clone(),
                            r#type: tag.kind,
                            count: tag.count,
                        });
                }
            }
        }
        Ok(result)
    }
}

pub(crate) async fn load(
    ctx: &Context<'_>,
    kind: DataType,
    id: &str,
) -> async_graphql::Result<Vec<Tag>> {
    Ok(ctx
        .data::<DataLoader<TagsLoader>>()?
        .load_one(TagKey {
            kind: kind.kind(),
            id: id.to_owned(),
        })
        .await?
        .unwrap_or_default())
}
