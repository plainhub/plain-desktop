//! NAS system/domain modules ported from the plainnas crate: storage
//! mounts/disks listing, block-device helpers, the USB automounter,
//! disk formatting, Samba shares, device info, app-update check, PDF
//! preview, the DLNA sender stack, SQLite devtools routing, temp values
//! and the file logger. Hosted behind the `nas` feature so the plainnas
//! shell (and future shells) share one implementation.

pub mod app_update;
pub mod automount;
pub mod blockdev;
pub mod consts;
pub mod device_info;
pub mod devtools_sqlite;
pub mod dlna;
pub mod format_disk;
pub mod log;
pub mod mounts;
pub mod pdf_preview;
pub mod samba;
pub mod storage_disks;
pub mod temp_store;
pub mod version;
pub mod xml_sax;
