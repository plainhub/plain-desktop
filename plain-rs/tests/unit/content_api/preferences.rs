use super::*;

#[test]
fn committed_preferences_emit_public_state_once_without_host_forwarding() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    let (events, mut receiver) = broadcast::channel(16);
    for (key, value, kind, payload) in [
        (
            "device_name",
            json!("Synthetic"),
            21,
            "\"Synthetic\"".to_string(),
        ),
        ("notification_filter", json!("{}"), 10, String::new()),
        (
            "pomodoro_settings",
            json!("{\"workDurationMin\":30}"),
            12,
            "{\"workDurationMin\":30}".to_string(),
        ),
    ] {
        assert!(set(&prefs, &events, true, key, value.clone()).unwrap());
        let invalidation = receiver.try_recv().unwrap();
        assert_eq!(invalidation.event_type, 10010);
        let public = receiver.try_recv().unwrap();
        assert_eq!(public.event_type, kind);
        assert_eq!(public.payload, payload);
        assert!(!public.is_host_event());
        assert!(receiver.try_recv().is_err());
        assert!(!set(&prefs, &events, true, key, value).unwrap());
        assert!(receiver.try_recv().is_err());
    }
}
