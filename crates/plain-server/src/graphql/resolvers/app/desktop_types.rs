use crate::assembly::enums::AppChannelType;
use crate::assembly::enums::DeviceType;
use crate::graphql::resolvers::app::types::Capability;
use crate::graphql::resolvers::device::permissions::Permission;
use async_graphql::SimpleObject;

#[derive(SimpleObject)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
}

#[derive(SimpleObject)]
pub struct App {
    pub client_id: String,
    pub url_token: String,
    pub http_port: i32,
    pub https_port: i32,
    pub app_dir: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub capabilities: Vec<Capability>,
    pub build_channel: AppChannelType,
    pub permissions: Vec<crate::assembly::enums::Permission>,
    pub downloads_dir: String,
    pub developer_mode: bool,
    pub debug: bool,
}
