//! GraphQL pairing mutations — browser ↔ Android-device local server path.
//!
//! These mutations expose the same `PairingManager` API as the Tauri
//! `commands::pairing` commands, so a browser running without the Tauri
//! runtime can drive pairing over HTTPS/GraphQL/WebSocket directly.
//! Status changes are pushed back as `WS_PAIRING_*` events; see
//! `lib.rs` for the bridge.
//!
//! Input shapes mirror plain-app's
//! `app/src/main/java/com/ismartcoding/plain/web/models/Pairing.kt` so the
//! same GraphQL operations work against either plain-web's local Rust server
//! or plain-app's Android HTTP server.

use async_graphql::{Context, Enum, ID, InputObject, Object, Result as GqlResult};
use std::sync::Arc;

use super::media::types::{Instant, Long};
use crate::api::context::AppCtx;
use crate::api::enums::DeviceType;
use crate::chat::pairing::protocol::PairingRequest;

#[derive(Clone, Copy, Eq, PartialEq, Enum)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum DiscoveryMethod {
    Lan,
    Ble,
    Qr,
}

/// Initiate pairing with a discovered LAN device. Mirrors plain-app's
/// `PairingDeviceInput` (see `app/.../web/models/Pairing.kt`).
#[derive(InputObject)]
#[graphql(rename_fields = "camelCase")]
pub struct PairingDeviceInput {
    pub id: ID,
    pub name: String,
    pub ips: Vec<String>,
    pub port: i32,
    pub device_type: DeviceType,
    pub version: String,
    pub platform: String,
    /// ISO-8601 string. We don't currently use it for the protocol, but
    /// accept it so the browser can pass the full discovered device.
    pub last_seen: Instant,
    pub discovery_methods: Vec<DiscoveryMethod>,
}

/// Incoming-pairing request payload. Mirrors plain-app's
/// `PairingRequestInput` (see `app/.../web/models/Pairing.kt`).
#[derive(InputObject)]
#[graphql(rename_fields = "camelCase")]
pub struct PairingRequestInput {
    pub from_id: ID,
    pub from_name: String,
    pub port: i32,
    pub device_type: DeviceType,
    pub ecdh_public_key: String,
    pub signature_public_key: String,
    pub timestamp: Long,
    pub ips: Vec<String>,
    /// Signature on the original PAIR_REQUEST — required for the responder
    /// to verify the requester. Empty string is rejected by the signature
    /// check; the browser always forwards the full request object.
    pub signature: String,
    /// Stamped on the receiver side; carried in the input so the
    /// responder knows which IP to POST the response back to.
    pub from_ip: String,
    /// Whether the requester's device supports Wi-Fi Aware (mirrors
    /// plain-app's `PairingRequestInput.awareSupported`). Forwarded to
    /// the protocol layer; the Tauri desktop build always sends `false`.
    pub aware_supported: bool,
}

#[derive(Default)]
pub struct PairingMutation;

#[Object]
impl PairingMutation {
    /// Initiate pairing with a discovered device. POSTs a PAIR_REQUEST to the
    /// target's `POST /nearby` endpoint. Completion (success / fail / timeout /
    /// cancel) is reported via `WS_PAIRING_*` push events.
    async fn pair_device(&self, ctx: &Context<'_>, input: PairingDeviceInput) -> GqlResult<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        // The Rust PairingManager.start_pairing only needs (id, name, ip,
        // port). The other input fields (ips, version, platform, last_seen)
        // are unused for the outgoing handshake — the response carries the
        // target's own fields back. We pick the best-subnet-match IP as the
        // POST target (mirrors plain-app's `getBestIp`); an empty result is
        // rejected by the network layer, surfacing as a `PAIRING_FAILED`
        // event.
        let target_ip = crate::mdns::host_responder::get_best_ip(&input.ips);
        c.chat.pairing.start_pairing(
            input.id.as_str(),
            &input.name,
            &target_ip,
            input.port as u16,
            c.https_port.load(std::sync::atomic::Ordering::Relaxed),
        );
        Ok(true)
    }

    /// Cancel an in-progress pairing we initiated. The remote peer receives
    /// a PAIR_CANCEL via `POST /nearby`; completion is reported via
    /// `WS_PAIRING_CANCELLED`.
    async fn cancel_pairing(&self, ctx: &Context<'_>, device_id: ID) -> GqlResult<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.chat.pairing.cancel_pairing(device_id.as_str());
        Ok(true)
    }

    /// Respond to an incoming PAIR_REQUEST that the user accepted or
    /// rejected. Accepting stores the peer and POSTs a PAIR_RESPONSE
    /// back to the requester at `input.from_ip`.
    async fn respond_to_pairing(
        &self,
        ctx: &Context<'_>,
        input: PairingRequestInput,
        accepted: bool,
    ) -> GqlResult<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let sender_ip = input.from_ip.clone();
        let req = PairingRequest {
            from_id: input.from_id.to_string(),
            from_name: input.from_name,
            port: input.port as u16,
            device_type: input.device_type.to_string(),
            ecdh_public_key: input.ecdh_public_key,
            signature_public_key: input.signature_public_key,
            timestamp: input.timestamp.0,
            ips: input.ips,
            signature: input.signature,
            aware_supported: input.aware_supported,
            from_ip: input.from_ip,
        };
        c.chat.pairing.respond_to_pairing(
            req,
            &sender_ip,
            accepted,
            c.https_port.load(std::sync::atomic::Ordering::Relaxed),
        );
        Ok(true)
    }
}
