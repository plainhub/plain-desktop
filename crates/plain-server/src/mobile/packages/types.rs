use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::Enum;
use async_graphql::ID;
use async_graphql::SimpleObject;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum PackageType {
    System,
    /// Anything the platform did not report as a pre-installed package.
    /// Misclassifying a user app as system would hide it from uninstall.
    #[default]
    User,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Certificate {
    pub issuer: String,
    pub subject: String,
    #[graphql(name = "serialNumber")]
    pub serial_number: String,
    #[graphql(name = "validFrom")]
    pub valid_from: Instant,
    #[graphql(name = "validTo")]
    pub valid_to: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Package {
    pub id: ID,
    pub name: String,
    pub r#type: PackageType,
    pub version: String,
    pub path: String,
    pub size: Long,
    pub certs: Vec<Certificate>,
    #[graphql(name = "installedAt")]
    pub installed_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct PackageStatus {
    pub id: ID,
    pub exists: bool,
    #[graphql(name = "updatedAt")]
    pub updated_at: Option<Instant>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct PackageInstallPending {
    pub id: ID,
    #[graphql(name = "updatedAt")]
    pub updated_at: Option<Instant>,
    #[graphql(name = "isNew")]
    pub is_new: bool,
}
