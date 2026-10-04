use super::{
    protocol::{PairingRequest, PairingResponse},
    timestamp_ok,
};
pub fn verify_request(request: &PairingRequest) -> bool {
    !request.from_id.is_empty()
        && timestamp_ok(request.timestamp)
        && crate::ed25519_verify(
            &request.signature_public_key,
            request.signature_data().as_bytes(),
            &request.signature,
        )
}
pub fn verify_response(response: &PairingResponse, actor: &str, expected: &str) -> bool {
    response.to_id == actor
        && response.from_id == expected
        && !actor.is_empty()
        && !expected.is_empty()
        && timestamp_ok(response.timestamp)
        && crate::ed25519_verify(
            &response.signature_public_key,
            response.signature_data().as_bytes(),
            &response.signature,
        )
}
