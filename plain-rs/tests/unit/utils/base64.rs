use super::*;

#[test]
fn rfc4648_vectors() {
    for (plain, encoded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64_encode(plain.as_bytes()), encoded, "encode {plain:?}");
        assert_eq!(
            base64_decode(encoded),
            plain.as_bytes(),
            "decode {encoded:?}"
        );
    }
    // The URL-safe alphabet swaps the two characters that are unsafe in
    // a URL, and decodes to exactly the same bytes.
    assert_eq!(base64_encode_url_safe(&[0xfb, 0xff, 0xbf]), "-_-_");
    assert_eq!(base64_decode("-_-_"), vec![0xfb, 0xff, 0xbf]);
}

#[test]
fn binary_round_trip_over_every_byte() {
    let all: Vec<u8> = (0..=255u8).collect();
    for encoded in [
        base64_encode(&all),
        base64_encode_url_safe(&all),
        base64_encode(&all).replace('=', ""),
    ] {
        assert_eq!(base64_decode(&encoded), all);
    }
}

#[test]
fn unpadded_input_decodes_like_padded_input() {
    assert_eq!(base64_decode("Zm9vYg"), b"foob".to_vec());
    assert_eq!(base64_decode("Zg"), b"f".to_vec());
}

#[test]
fn junk_is_rejected_instead_of_decoding_to_garbage() {
    // The old decoder mapped every unknown character to 0, so a
    // corrupted key still "decoded" to a full-length buffer and passed
    // length checks downstream.
    for junk in ["Zm9v!!!!", "Zm9v Yg==", "Zg==Zg", "@@@@", "A"] {
        assert!(
            base64_decode_checked(junk).is_err(),
            "{junk:?} must be rejected"
        );
    }
    assert!(base64_decode_checked("Zm9v!!!!").is_err());
    // Mixed alphabets in one string are a mistake, not a URL-safe value.
    assert!(base64_decode_checked("ab-_cd+/").is_err());
    // Report where it went wrong.
    assert_eq!(
        base64_decode_checked("Zm9v!"),
        Err(Invalid {
            offset: 4,
            found: '!'
        })
    );
}
