//! mDNS (RFC 6762) protocol stack for `_plainapp._tcp.local` discovery.
//!
//! - [`packet_codec`]: DNS wire-format encode/decode (queries, records, names)
//! - [`service_info`]: service model + parsed record accessors
//! - [`service_response_builder`]: builds answers for the published service
//! - [`host_responder`]: shared 5353 socket, multicast join/send, A-record
//!   hostname responder
//! - [`service_browser`]: browsing state machine that resolves instances into
//!   complete devices

pub mod host_responder;
pub mod packet_codec;
pub mod service_browser;
pub mod service_info;
pub mod service_response_builder;
