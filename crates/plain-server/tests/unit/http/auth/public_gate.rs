use super::*;
use crate::graphql::executor::RequestOrigin;
use plain_rs::prefs::Prefs;
use plain_rs::prefs::system::keys::API_PERMISSIONS;
use std::sync::Arc;

fn prefs_with(permissions: &[&str]) -> Arc<Prefs> {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs
        .set(
            API_PERMISSIONS,
            permissions
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    prefs
}

/// A remote caller with no granted permission is refused, exactly as before.
#[test]
fn remote_caller_without_permission_is_refused() {
    use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema};
    struct Query;
    #[Object]
    impl Query {
        async fn probe(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
            require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &["STORAGE"])?;
            Ok(true)
        }
    }
    let prefs = prefs_with(&[]);
    for origin in [RequestOrigin::Remote] {
        let schema: Schema<Query, EmptyMutation, EmptySubscription> =
            Schema::build(Query, EmptyMutation, EmptySubscription).finish();
        let response = tokio::runtime::Runtime::new().unwrap().block_on(async {
            schema
                .execute(
                    async_graphql::Request::new("query { probe }")
                        .data(prefs.clone())
                        .data(origin),
                )
                .await
        });
        assert!(
            response.errors.iter().any(|e| e.message == "no_permission"),
            "remote caller must still be refused, got {:?}",
            response.errors
        );
    }
}

/// The app calling the same field over the native bridge is not judged by the
/// web opt-in list — otherwise it could not browse its own files once it stops
/// going through loopback HTTP.
#[test]
fn local_caller_bypasses_the_web_permission_list() {
    use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema};
    struct Query;
    #[Object]
    impl Query {
        async fn probe(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
            require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &["STORAGE"])?;
            Ok(true)
        }
    }
    let prefs = prefs_with(&[]);
    let schema: Schema<Query, EmptyMutation, EmptySubscription> =
        Schema::build(Query, EmptyMutation, EmptySubscription).finish();
    let response = tokio::runtime::Runtime::new().unwrap().block_on(async {
        schema
            .execute(
                async_graphql::Request::new("query { probe }")
                    .data(prefs.clone())
                    .data(RequestOrigin::Local),
            )
            .await
    });
    assert!(
        response.errors.is_empty(),
        "local caller must pass, got {:?}",
        response.errors
    );
}

/// A remote caller that *was* granted the permission still passes.
#[test]
fn remote_caller_with_permission_is_allowed() {
    use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema};
    struct Query;
    #[Object]
    impl Query {
        async fn probe(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
            require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &["STORAGE"])?;
            Ok(true)
        }
    }
    let prefs = prefs_with(&["STORAGE"]);
    let schema: Schema<Query, EmptyMutation, EmptySubscription> =
        Schema::build(Query, EmptyMutation, EmptySubscription).finish();
    let response = tokio::runtime::Runtime::new().unwrap().block_on(async {
        schema
            .execute(
                async_graphql::Request::new("query { probe }")
                    .data(prefs.clone())
                    .data(RequestOrigin::Remote),
            )
            .await
    });
    assert!(
        response.errors.is_empty(),
        "granted remote caller must pass, got {:?}",
        response.errors
    );
}

#[test]
fn write_contact_and_call_permissions_imply_read_access() {
    let configured = ["WRITE_CONTACTS".to_owned(), "WRITE_CALL_LOG".to_owned()]
        .into_iter()
        .collect();
    assert!(api_enabled(
        &["READ_CONTACTS".into(), "WRITE_CALL_LOG".into()],
        &configured
    ));
    assert!(!api_enabled(&["READ_SMS".into()], &configured));
}

#[test]
fn every_requested_permission_must_be_configured() {
    let configured = ["READ_SMS".to_owned()].into_iter().collect();
    assert!(api_enabled(&["READ_SMS".into()], &configured));
    assert!(!api_enabled(
        &["READ_SMS".into(), "SEND_SMS".into()],
        &configured
    ));
}
