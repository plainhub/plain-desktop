//! Local GraphQL surface — shared implementation in plain-rs
//! (`local_api::{context, executor, schema}`).
#[allow(unused_imports)]
pub use plain_rs::local_api::context::{
    AppCtx, WS_BOOKMARK_UPDATED, WS_DEVICE_NAME_UPDATED, WS_DOWNLOAD_PROGRESS,
    WS_NEARBY_DEVICE_FOUND, WS_NEARBY_DEVICE_UNREACHABLE, WS_NEARBY_DISCOVERY_STARTED,
    WS_NEARBY_DISCOVERY_STOPPED, WS_PAIRING_CANCELLED, WS_PAIRING_FAILED,
    WS_PAIRING_REQUEST_RECEIVED, WS_PAIRING_STARTED, WS_PAIRING_SUCCESS, WS_UPLOAD_MERGE_RESULT,
    WsEvent, encode_ws_event,
};
#[allow(unused_imports)]
pub use plain_rs::local_api::executor::execute_graphql;
pub use plain_rs::local_api::schema::{self, LocalSchema, build_schema};
