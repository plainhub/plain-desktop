pub mod callbacks;
pub mod graphql;
pub mod management;
pub mod resources;
pub mod sockets;
pub mod uploads;
pub mod views;

#[derive(Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NativeTarget {
    Local {},
    Peer { peer_id: String },
}
