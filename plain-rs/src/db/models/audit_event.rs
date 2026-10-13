#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEventRow {
    pub id: String,
    pub r#type: String,
    pub message: String,
    pub client_id: String,
    pub created_at: String,
}
