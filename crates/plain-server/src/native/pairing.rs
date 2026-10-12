use crate::assembly::server::ServerState;
use crate::services::discovery::pairing_runtime;
use plain_rs::peers::pairing::{protocol::PairingRequest, sessions::Target};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentity {
    pub client_id: String,
    pub device_name: String,
    pub public_key: String,
}

#[derive(Clone)]
pub struct NativePairing {
    state: ServerState,
}

impl NativePairing {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub fn identity(&self) -> Result<DeviceIdentity, String> {
        let client_id = self.state.native_client_id()?;
        let user = plain_rs::prefs::user::snapshot(&self.state.prefs)
            .map_err(|error| error.to_string())?;
        let keypair = crate::services::peers::peer_wire::signing_keypair(&self.state.prefs)
            .map_err(|error| error.to_string())?;
        let public_key = plain_rs::crypto::ed25519_public_from_keypair(&keypair)
            .ok_or("invalid signing identity")?;
        Ok(DeviceIdentity {
            client_id,
            device_name: user.device_name,
            public_key: plain_rs::utils::base64::base64_encode(&public_key),
        })
    }

    pub async fn start(
        &self,
        target: Target,
        ips: Vec<String>,
        methods: Vec<String>,
    ) -> Result<(), String> {
        self.state.native_client_id()?;
        pairing_runtime::start_device(&self.state, target, ips, methods)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn cancel(&self, device_id: &str) -> Result<(), String> {
        self.state.native_client_id()?;
        pairing_runtime::execute(
            &self.state,
            pairing_runtime::Request::Cancel {
                id: device_id.to_owned(),
                generation: None,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn respond(
        &self,
        request: PairingRequest,
        sender_ip: &str,
        accepted: bool,
    ) -> Result<(), String> {
        self.state.native_client_id()?;
        let target = self
            .state
            .pairing
            .incoming_target(&request.from_id, &request.signature)
            .ok_or("incoming pairing unavailable")?;
        if target.device_ip != sender_ip {
            return Err("pairing sender does not match incoming request".into());
        }
        pairing_runtime::respond_device(&self.state, request, accepted)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn set_device_name(&self, name: String) -> Result<bool, String> {
        self.state.native_client_id()?;
        crate::graphql::resolvers::settings::state::apply_user_patch(
            crate::graphql::resolvers::settings::state::UserPatchEnv::from_server(&self.state),
            plain_rs::prefs::user::UserSettingsPatch {
                device_name: Some(name),
                ..Default::default()
            },
        )
        .await
        .map_err(|error| error.message)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/pairing.rs"]
mod tests;
