use super::*;
use crate::base64_decode;
use crate::xchacha_decrypt;

/// `make_file_id` (the `&str` variant) is used for text-message
/// link-preview paths. Decryption should give back the bare
/// `imageLocalPath` (not JSON-wrapped).
#[test]
fn make_file_id_roundtrips_through_fs_decrypt() {
    let token_raw = [7u8; 32];
    let token_b64 = crate::base64_encode(&token_raw);

    let path = "app://Pictures/foo.png";
    let fid = make_file_id(path, &token_b64);

    let plaintext = xchacha_decrypt(&token_b64, &base64_decode(&fid))
        .expect("decrypt must succeed for text link-preview path");
    assert_eq!(std::str::from_utf8(&plaintext).unwrap(), path);
}
