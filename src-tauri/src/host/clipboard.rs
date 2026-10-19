use std::future::Future;
use tokio::sync::watch;

#[derive(Clone, PartialEq, Eq, serde::Serialize)]
pub(super) struct Sample {
    pub text: String,
    pub sensitive: bool,
}

pub(super) fn sensitive() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2_app_kit::NSPasteboard::generalPasteboard()
            .types()
            .is_some_and(|types| {
                types
                    .iter()
                    .any(|name| name.to_string() == "org.nspasteboard.ConcealedType")
            })
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub(super) async fn observe<Read, Reading, Publish, Publishing>(
    mut running: watch::Receiver<bool>,
    mut read: Read,
    mut publish: Publish,
) where
    Read: FnMut() -> Reading,
    Reading: Future<Output = Result<Option<Sample>, String>>,
    Publish: FnMut(Sample) -> Publishing,
    Publishing: Future<Output = Result<(), String>>,
{
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut previous = None;
    while *running.borrow_and_update() {
        tokio::select! {
            biased;
            result = running.changed() => {
                if result.is_err() { break; }
                continue;
            },
            _ = interval.tick() => {},
        }
        let sample = match read().await {
            Ok(sample) => sample,
            Err(error) => {
                log::warn!("clipboard observation failed: {error}");
                continue;
            }
        };
        if !*running.borrow() {
            break;
        }
        if sample == previous {
            continue;
        }
        if let Some(value) = sample.as_ref() {
            if let Err(error) = publish(value.clone()).await {
                log::warn!("clipboard report failed: {error}");
                continue;
            }
        }
        previous = sample;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/host/clipboard.rs"]
mod tests;
