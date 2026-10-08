use super::{host::Host, public_calls::PhoneGeo};
use serde::Deserialize;
use serde_json::json;
use std::{collections::VecDeque, sync::Mutex};

#[derive(Deserialize)]
pub(super) struct LocaleFacts {
    pub region: String,
    pub locale: String,
    pub available: bool,
}
#[derive(Default)]
pub(super) struct Runtime {
    cache: Mutex<VecDeque<(String, Option<PhoneGeo>)>>,
}
impl Runtime {
    pub async fn lookup(
        &self,
        host: &Host,
        number: &str,
        locale: &LocaleFacts,
    ) -> Option<PhoneGeo> {
        if !locale.available || number.trim().is_empty() {
            return None;
        }
        let key = format!("{}\0{}\0{}", locale.region, locale.locale, number);
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(index) = cache.iter().position(|(cached, _)| cached == &key) {
                let entry = cache.remove(index).unwrap();
                let result = entry.1.clone();
                cache.push_back(entry);
                return result;
            }
        }
        let result = self.resolve(host, number, locale).await;
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|(cached, _)| cached != &key);
        cache.push_back((key, result.clone()));
        while cache.len() > 1024 {
            cache.pop_front();
        }
        result
    }
    async fn resolve(&self, host: &Host, number: &str, locale: &LocaleFacts) -> Option<PhoneGeo> {
        // Parsing and classification belong to the platform: it owns the
        // libphonenumber data for the device's region, and iOS has none at
        // all. This used to parse the number here and hand back a country
        // code plus national number just so the host could rebuild the very
        // same number object.
        let facts = host
            .call(
                "systemPhoneMetadata",
                json!({
                    "number":number,"region":locale.region,"locale":locale.locale
                }),
            )
            .await
            .ok()?;
        let country = facts["country"].as_str().unwrap_or_default();
        if country.is_empty() {
            return None;
        }
        Some(PhoneGeo {
            country: country.to_owned(),
            number_type: facts["numberType"].as_str().unwrap_or_default().to_owned(),
            carrier: facts["carrier"].as_str().unwrap_or_default().to_owned(),
            description: facts["description"].as_str().unwrap_or_default().to_owned(),
        })
    }
}

#[cfg(test)]
#[path="../../tests/unit/content_api/phone_geo.rs"]
mod tests;
