//! Local GraphQL server — entry point.
//!
//! Sub-modules:
//!   context  — WsEvent, AppCtx
//!   executor — execute_graphql, stub pre-filter
//!   schema   — async-graphql types, QueryRoot, MutationRoot

pub mod context;
pub mod executor;
pub mod schema;

pub use context::{
    AppCtx, WS_NEARBY_DEVICE_FOUND, WS_NEARBY_DEVICE_UNREACHABLE, WS_NEARBY_DISCOVERY_STARTED,
    WS_NEARBY_DISCOVERY_STOPPED, WS_PEER_STATUS_UPDATED, WsEvent, encode_ws_event,
};
pub use executor::execute_graphql;
pub use schema::{LocalSchema, build_schema};
