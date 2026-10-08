use super::*;
use serde_json::json;
#[test]
fn shared_merge_states_keep_the_desktop_graphql_contract() {
    assert_eq!(task(json!({"status":"NONE"})).status,MergeTaskStatus::None);
    assert_eq!(task(json!({"status":"MERGING"})).status,MergeTaskStatus::Merging);
    let done=task(json!({"status":"DONE","value":"a.jpg","mergedSize":42}));assert_eq!(done.status,MergeTaskStatus::Done);assert_eq!(done.value.as_deref(),Some("a.jpg"));assert_eq!(done.merged_size.map(|v|v.0),Some(42));assert!(done.error.is_none());
    let failed=task(json!({"status":"FAILED","error":"boom"}));assert_eq!(failed.status,MergeTaskStatus::Failed);assert_eq!(failed.error.as_deref(),Some("boom"));assert!(failed.value.is_none());
}
