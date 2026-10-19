use super::*;
use std::sync::{Arc, Mutex};

fn sample(text: &str, sensitive: bool) -> Sample {
    Sample {
        text: text.into(),
        sensitive,
    }
}

#[tokio::test]
async fn repeated_samples_and_read_failures_do_not_duplicate_reports_and_disabling_resets_sampling()
{
    let (running, stopped) = watch::channel(true);
    let mut samples = std::collections::VecDeque::from([
        Ok(Some(sample("first", false))),
        Ok(Some(sample("first", false))),
        Err("temporary read failure".to_owned()),
        Ok(Some(sample("first", false))),
        Ok(Some(sample("first", true))),
        Ok(None),
        Ok(Some(sample("first", true))),
    ]);
    let reports = Arc::new(Mutex::new(Vec::new()));
    let published = reports.clone();
    observe(
        stopped,
        move || {
            let next = samples.pop_front().unwrap_or_else(|| {
                running.send_replace(false);
                Ok(None)
            });
            std::future::ready(next)
        },
        move |sample| {
            published.lock().unwrap().push(sample);
            std::future::ready(Ok(()))
        },
    )
    .await;
    let actual = reports.lock().unwrap();
    assert_eq!(actual.len(), 3);
    assert_eq!(actual[0].text, "first");
    assert!(!actual[0].sensitive);
    assert!(actual[1].sensitive && actual[2].sensitive);
}

#[tokio::test]
async fn a_failed_report_is_retried_with_the_current_sample() {
    let (running, stopped) = watch::channel(true);
    let attempts = Arc::new(Mutex::new(0));
    let published = attempts.clone();
    observe(
        stopped,
        || std::future::ready(Ok(Some(sample("retry", false)))),
        move |_| {
            let mut attempts = published.lock().unwrap();
            *attempts += 1;
            let result = if *attempts == 1 {
                Err("temporary report failure".into())
            } else {
                running.send_replace(false);
                Ok(())
            };
            std::future::ready(result)
        },
    )
    .await;
    assert_eq!(*attempts.lock().unwrap(), 2);
}

#[tokio::test]
async fn shutdown_waits_for_the_owned_read_and_never_publishes_its_late_sample() {
    let (running, stopped) = watch::channel(true);
    let (began, reading) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let mut began = Some(began);
    let mut released = Some(released);
    let observation = tokio::spawn(observe(
        stopped,
        move || {
            began.take().unwrap().send(()).unwrap();
            let released = released.take().unwrap();
            async move {
                released.await.unwrap();
                Ok(Some(sample("late", false)))
            }
        },
        |_| async { panic!("a stopped host must not publish") },
    ));
    reading.await.unwrap();
    running.send_replace(false);
    assert!(!observation.is_finished());
    release.send(()).unwrap();
    observation.await.unwrap();
}

#[tokio::test]
async fn stopped_or_dropped_sessions_never_read_clipboard_contents() {
    for active in [false, true] {
        let (running, stopped) = watch::channel(active);
        drop(running);
        observe(
            stopped,
            || async { panic!("closed host must not sample") },
            |_| async { Ok(()) },
        )
        .await;
    }
}
