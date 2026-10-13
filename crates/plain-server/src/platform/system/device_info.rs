pub use plain_rs::platform::linux::device_info::*;

pub async fn collect() -> anyhow::Result<DeviceInfo> {
    plain_rs::platform::linux::device_info::collect(
        super::version::version(),
        &super::version::full_version(),
    )
}
