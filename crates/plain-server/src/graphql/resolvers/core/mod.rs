//! App-only core lifecycle roots.
//!
//! These answer "is this device's own HTTP server up" — the question the
//! desktop-access diagnostics page asks after it starts the listener. It is
//! deliberately *not* on the contract schema: a remote client asking a remote
//! device whether its listener is healthy has no use for the answer, and the
//! check is a self-probe of this process rather than a capability.
//!
//! The probe itself stays in Rust (`lifecycle::probe`): the app never issues
//! the loopback request, it just asks for the verdict.

use async_graphql::{Context, Object};
use std::sync::Arc;

use crate::assembly::server::ServerState;

#[derive(Default)]
pub struct CoreQuery;

#[Object]
impl CoreQuery {
    /// Whether this device's HTTP and HTTPS listeners answer their readiness
    /// probe. False (rather than an error) when no server is running.
    async fn core_http_server_healthy(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        Ok(
            crate::assembly::lifecycle::probe(&ctx.data_unchecked::<ServerState>())
                .await
                .is_ok(),
        )
    }

    /// The failure reason, or null when healthy.
    async fn core_http_server_error(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<String>> {
        Ok(
            crate::assembly::lifecycle::probe(&ctx.data_unchecked::<ServerState>())
                .await
                .err(),
        )
    }

    /// The ports this device is listening on, for the diagnostics page.
    async fn core_http_ports(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<CorePorts>> {
        let state = ctx.data_unchecked::<ServerState>();
        let guard = state.public_server.lock().await;
        Ok(guard.as_ref().map(|server| CorePorts {
            http: server.listeners.http_port,
            https: server.listeners.https_port,
        }))
    }
}

pub struct CorePorts {
    pub http: u16,
    pub https: u16,
}

#[Object]
impl CorePorts {
    /// The plain HTTP port, used for LAN access and the DLNA media URL.
    async fn http(&self) -> u16 {
        self.http
    }

    /// The TLS port, advertised over mDNS and shown by QR pairing.
    async fn https(&self) -> u16 {
        self.https
    }
}
