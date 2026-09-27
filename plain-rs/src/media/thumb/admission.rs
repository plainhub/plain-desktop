//! Two-tier admission control for thumbnail generation.
//!
//! The old pipeline capped concurrency at `min(cpus, 8)` permits to avoid
//! OOM (8 × full-size decoded bitmaps). That conflated two resources:
//!
//! - **CPU**: decode is compute-bound; ~2 permits per core saturates the box
//!   while leaving headroom for I/O waits.
//! - **Decoded-pixel memory**: the real reason the old cap existed. Pricing
//!   it per-request (from the sniffed header) lets hundreds of *small*
//!   generations run concurrently while a handful of 50 MP decodes are held
//!   back by a byte budget, not by a global counter.

use std::sync::OnceLock;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// One memory-permit unit = 64 KiB of estimated decoded bitmap.
pub const MEM_UNIT: u64 = 64 * 1024;

/// Hard ceiling on decoded source pixels, any format (decompression-bomb
/// guard). Scaled JPEG decodes are priced by their *scaled* output, so
/// huge-but-shrinking JPEGs stay allowed; this guards full decodes.
pub const MAX_SOURCE_PIXELS: u64 = 300_000_000;

pub struct Admission {
    cpu: std::sync::Arc<Semaphore>,
    mem: std::sync::Arc<Semaphore>,
    /// Total memory budget in permit units (the semaphore alone cannot tell
    /// "fully used" from "configured total").
    mem_total: u64,
}

impl Admission {
    pub fn new(cpu_permits: usize, mem_budget_mb: u32) -> Self {
        let total_units = units_for(u64::from(mem_budget_mb.max(1)) * 1024 * 1024);
        Admission {
            cpu: std::sync::Arc::new(Semaphore::new(cpu_permits.max(1))),
            mem: std::sync::Arc::new(Semaphore::new(total_units.min(u32::MAX as u64) as usize)),
            mem_total: total_units,
        }
    }

    pub fn mem_budget_mb(&self) -> u32 {
        (self.mem_total * MEM_UNIT / (1024 * 1024)) as u32
    }

    /// Acquire CPU + memory permits for a job whose decoded bitmap is
    /// estimated at `est_bytes`. Rejects a job that could never fit the
    /// budget on its own; otherwise queues fairly behind other holders.
    pub async fn acquire(&self, est_bytes: u64) -> anyhow::Result<Permits> {
        let units = units_for(est_bytes) as usize;
        if units as u64 > self.mem_total && units > 0 {
            anyhow::bail!(
                "thumbnail decode estimate {est_bytes} exceeds memory budget {}",
                self.mem_budget_mb()
            );
        }
        // CPU first, then memory: memory holders never wait on CPU, so this
        // ordering cannot deadlock.
        let cpu = self
            .cpu
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("cpu semaphore closed"))?;
        let mem = if units > 0 {
            Some(
                self.mem
                    .clone()
                    .acquire_many_owned(units as u32)
                    .await
                    .map_err(|_| anyhow::anyhow!("mem semaphore closed"))?,
            )
        } else {
            None
        };
        Ok(Permits {
            _cpu: cpu,
            _mem: mem,
        })
    }
}

/// Estimated byte cost → memory-permit units. Floors: jobs under one unit
/// are free — that is the point of pricing instead of counting.
fn units_for(est_bytes: u64) -> u64 {
    est_bytes / MEM_UNIT
}

/// Per-job RAII permits.
pub struct Permits {
    _cpu: OwnedSemaphorePermit,
    _mem: Option<OwnedSemaphorePermit>,
}

static GLOBAL: OnceLock<Admission> = OnceLock::new();

fn build(cfg: &crate::media::config::Config) -> Admission {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let cpu_permits = (cpus * 2).clamp(4, 64);
    // Missing/invalid key (get_int → 0) falls back to the 2 GB default.
    let raw = cfg.get_int("thumbnails.mem_budget_mb");
    let mb = if raw <= 0 {
        2048
    } else {
        raw.clamp(128, 16_384)
    } as u32;
    Admission::new(cpu_permits, mb)
}

/// Configure the global admission controller from `[thumbnails]` config.
/// Called once at startup; the first call wins.
pub fn init_from_config(cfg: &crate::media::config::Config) {
    let _ = GLOBAL.set(build(cfg));
}

pub fn global() -> &'static Admission {
    GLOBAL.get_or_init(|| build(&crate::media::config::Config::default()))
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/admission.rs"]
mod tests;
