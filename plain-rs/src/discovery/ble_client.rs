//! Client-side BLE request/response driver — Rust owns the state machine.
//!
//! A peer request is: allocate a request id, chunk the message to the GATT
//! write limit, write the frames, then read notifications until the response
//! assembles. That sequencing, its deadlines, and its invariants (response
//! direction, matching request id) are protocol logic and live here. The
//! platform layer supplies only the two I/O primitives, so Android
//! (BluetoothGatt), iOS (CoreBluetooth), and a future Linux BlueZ host share
//! one driver.
//!
//! Replaces the Kotlin `BleDeviceApi` state machine. The I/O closures map onto
//! a Rust→host call (or a local GATT call on a native platform), so the state
//! machine stays portable while the radio stays in the platform layer.

use crate::protocols::ble::{self, Assembler};
use anyhow::{Result, ensure};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// Time budget for a whole request/response exchange.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// Time budget for one notification while assembling a response.
pub const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(15);

/// Monotonic request-id source. Starts at 1 so the first id is non-zero.
#[derive(Default)]
struct RequestIds(Mutex<u32>);

impl RequestIds {
    fn next(&self) -> Result<u32> {
        let mut id = self.0.lock().unwrap();
        *id = id.wrapping_add(1);
        ensure!(*id != 0 && *id != u32::MAX, "BLE request IDs exhausted");
        Ok(*id)
    }
}

#[derive(Default)]
pub struct Client {
    ids: RequestIds,
}

impl Client {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send `message` and return the raw response bytes once they assemble.
    ///
    /// * `write(limit, frame)` writes one framed chunk; `false` rejects it.
    /// * `next_notification(timeout)` yields the next inbound frame, or `None`
    ///   when the peer stays silent past the timeout.
    pub async fn request<W, WF, N, NF>(
        &self,
        message: &[u8],
        write_limit: usize,
        mut write: W,
        mut next_notification: N,
    ) -> Result<Vec<u8>>
    where
        W: FnMut(usize, Vec<u8>) -> WF,
        WF: std::future::Future<Output = Result<bool>>,
        N: FnMut(Duration) -> NF,
        NF: std::future::Future<Output = Result<Option<Vec<u8>>>>,
    {
        let request_id = self.ids.next()?;
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        ensure!(write_limit > ble::HEADER, "Invalid BLE write limit");

        let mut sequence = 0u32;
        loop {
            match ble::frame(message, request_id, false, sequence, write_limit)? {
                Some(frame) => {
                    ensure!(write(write_limit, frame).await?, "BLE request write failed");
                    sequence = sequence
                        .checked_add(1)
                        .ok_or_else(|| anyhow::anyhow!("BLE sequence overflow"))?;
                }
                None => break,
            }
        }

        let mut assembly = Assembler::default();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            ensure!(!remaining.is_zero(), "BLE request timed out");
            let frame = next_notification(remaining.min(NOTIFICATION_TIMEOUT))
                .await?
                .ok_or_else(|| anyhow::anyhow!("BLE notification timed out"))?;
            if let Some(result) = assembly.push(&frame)? {
                let info = assembly.info();
                ensure!(info >> 32 & 1 == 1, "Unexpected BLE response direction");
                ensure!(info as u32 == request_id, "Unexpected BLE response");
                return Ok(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    /// Shared fake peer: collects writes and serves queued response frames.
    #[derive(Clone, Default)]
    struct FakePeer {
        written: Rc<RefCell<Vec<Vec<u8>>>>,
        inbox: Rc<RefCell<VecDeque<Vec<u8>>>>,
    }

    impl FakePeer {
        fn with_inbox(frames: Vec<Vec<u8>>) -> Self {
            Self {
                written: Rc::new(RefCell::new(Vec::new())),
                inbox: Rc::new(RefCell::new(frames.into())),
            }
        }
    }

    /// Builds the response frames a peer would notify back for `request_id`.
    fn response_frames(message: &[u8], request_id: u32, limit: usize) -> Vec<Vec<u8>> {
        let size = limit - ble::HEADER;
        let mut out = Vec::new();
        let mut sequence = 0u32;
        loop {
            let start = (sequence as usize) * size;
            if start >= message.len() {
                break;
            }
            out.push(
                ble::frame(message, request_id, true, sequence, limit)
                    .unwrap()
                    .unwrap(),
            );
            sequence += 1;
        }
        out
    }

    #[tokio::test]
    async fn request_writes_and_reassembles() {
        let client = Client::new();
        let request = ble::message(&[], b"request").unwrap();
        let response = ble::message(&[], b"response body").unwrap();
        let peer = FakePeer::with_inbox(response_frames(&response, 1, 32));
        let written = peer.written.clone();
        let result = client
            .request(
                &request,
                32,
                {
                    let peer = peer.clone();
                    move |_, frame| {
                        let peer = peer.clone();
                        async move {
                            peer.written.borrow_mut().push(frame);
                            Ok(true)
                        }
                    }
                },
                move |_| {
                    let peer = peer.clone();
                    async move { Ok(peer.inbox.borrow_mut().pop_front()) }
                },
            )
            .await
            .unwrap();
        assert_eq!(result, response);
        assert!(!written.borrow().is_empty());
    }

    #[tokio::test]
    async fn rejects_a_response_for_another_request() {
        let client = Client::new();
        let request = ble::message(&[], b"request").unwrap();
        // The first id handed out is 1; answer with a mismatched one.
        let peer = FakePeer::with_inbox(response_frames(&ble::message(&[], b"x").unwrap(), 99, 32));
        assert!(
            client
                .request(
                    &request,
                    32,
                    move |_, _| async { Ok(true) },
                    move |_| {
                        let peer = peer.clone();
                        async move { Ok(peer.inbox.borrow_mut().pop_front()) }
                    },
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn times_out_without_a_response() {
        let client = Client::new();
        let request = ble::message(&[], b"request").unwrap();
        let peer = FakePeer::default();
        assert!(
            client
                .request(
                    &request,
                    32,
                    move |_, _| async { Ok(true) },
                    move |_| {
                        let peer = peer.clone();
                        async move { Ok(peer.inbox.borrow_mut().pop_front()) }
                    },
                )
                .await
                .is_err()
        );
    }
}