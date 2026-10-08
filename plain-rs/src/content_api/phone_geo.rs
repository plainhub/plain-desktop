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
        let number = phonenumber::parse(locale.region.parse().ok(), number).ok()?;
        if !number.is_valid() {
            return None;
        }
        let kind = number.number_type(&phonenumber::metadata::DATABASE);
        let number_type = match kind {
            phonenumber::Type::FixedLine => "FIXED_LINE",
            phonenumber::Type::Mobile => "MOBILE",
            phonenumber::Type::FixedLineOrMobile => "FIXED_LINE_OR_MOBILE",
            phonenumber::Type::TollFree => "TOLL_FREE",
            phonenumber::Type::PremiumRate => "PREMIUM_RATE",
            phonenumber::Type::SharedCost => "SHARED_COST",
            phonenumber::Type::PersonalNumber => "PERSONAL_NUMBER",
            phonenumber::Type::Voip => "VOIP",
            phonenumber::Type::Pager => "PAGER",
            phonenumber::Type::Uan => "UAN",
            phonenumber::Type::Voicemail => "VOICEMAIL",
            _ => "",
        };
        let code = number.code().value();
        let facts = host.call("systemPhoneMetadata", json!({
            "countryCode":code,"nationalNumber":number.national().to_string(),"locale":locale.locale,
            "includeCarrier":matches!(kind,phonenumber::Type::Mobile|phonenumber::Type::FixedLineOrMobile|phonenumber::Type::Pager),
        })).await.ok()?;
        Some(PhoneGeo {
            country: number
                .country()
                .id()
                .map(|country| country.as_ref().to_owned())
                .unwrap_or_default(),
            number_type: number_type.to_owned(),
            carrier: facts["carrier"].as_str().unwrap_or_default().to_owned(),
            description: facts["description"].as_str().unwrap_or_default().to_owned(),
        })
    }
}

#[cfg(test)]
#[path="../../tests/unit/content_api/phone_geo.rs"]
mod tests;
