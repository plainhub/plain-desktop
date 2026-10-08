use super::{message, parts, string, Reader};
use anyhow::{bail, ensure, Result};

#[derive(Debug, PartialEq)]
pub enum GatewayRequest {
    Auth {
        client_id: String,
        client_name: String,
        password: String,
    },
    Graphql {
        client_id: String,
        body: Vec<u8>,
    },
}
impl GatewayRequest {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut metadata = Vec::new();
        let body = match self {
            Self::Auth {
                client_id,
                client_name,
                password,
            } => {
                ensure!(!client_id.is_empty(), "Missing BLE client ID");
                validate_password(password)?;
                metadata.push(3);
                string(&mut metadata, client_id)?;
                string(&mut metadata, client_name)?;
                password.as_bytes()
            }
            Self::Graphql { client_id, body } => {
                ensure!(!client_id.is_empty(), "Missing BLE client ID");
                metadata.push(4);
                string(&mut metadata, client_id)?;
                body.as_slice()
            }
        };
        message(&metadata, body)
    }
    pub fn decode(data: &[u8]) -> Result<Self> {
        let (metadata, body) = parts(data)?;
        let mut reader = Reader(metadata);
        let operation = reader.take(1)?[0];
        let client_id = reader.string()?;
        ensure!(!client_id.is_empty(), "Missing BLE client ID");
        let result = match operation {
            3 => {
                let client_name = reader.string()?;
                let password = std::str::from_utf8(body)?.to_owned();
                validate_password(&password)?;
                Self::Auth {
                    client_id,
                    client_name,
                    password,
                }
            }
            4 => Self::Graphql {
                client_id,
                body: body.to_vec(),
            },
            _ => bail!("Unknown BLE gateway operation"),
        };
        ensure!(reader.0.is_empty(), "Trailing BLE metadata");
        Ok(result)
    }
}
fn validate_password(password: &str) -> Result<()> {
    ensure!(
        password.len() == 128
            && password
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid BLE password digest"
    );
    Ok(())
}
#[cfg(test)]
#[path = "../../../tests/unit/content_api/ble_gateway.rs"]
mod tests;
