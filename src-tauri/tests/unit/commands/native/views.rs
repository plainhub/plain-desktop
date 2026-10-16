use super::*;

#[test]
fn reloading_one_view_closes_its_previous_requests_and_preserves_other_views() {
    let views = NativeViews::default();
    let old = views.scope("main");
    let other = views.scope("preview");
    views.replace("main", true).unwrap();
    let current = views.scope("main");
    assert!(old.is_closed());
    assert!(!current.is_closed());
    assert!(!other.is_closed());
    old.close();
    assert!(!current.is_closed());
    views.replace("main", false).unwrap();
    assert!(current.is_closed());
    assert!(!other.is_closed());
}

#[test]
fn a_queued_old_document_request_cannot_use_a_reloaded_or_other_view_scope() {
    let views = NativeViews::default();
    let old = views.scope("main");
    assert!(views.existing_scope("main", old.id()).is_ok());
    assert!(views.existing_scope("preview", old.id()).is_err());
    views.replace("main", true);
    assert!(views.existing_scope("main", old.id()).is_err());
    let fresh = views.scope("main");
    assert!(views.existing_scope("main", fresh.id()).is_ok());
    views.replace("main", false);
    assert!(views.existing_scope("main", fresh.id()).is_err());
}
