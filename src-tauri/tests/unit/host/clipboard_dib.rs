use super::dib_v5;

#[test]
fn dib_preserves_alpha_and_pixel_order() {
    let rgba = [
        1, 2, 3, 4, 10, 20, 30, 40, 100, 120, 140, 160, 180, 200, 220, 240,
    ];
    let dib = dib_v5(2, 2, &rgba).unwrap();
    let field = |offset| u32::from_le_bytes(dib[offset..offset + 4].try_into().unwrap());
    assert_eq!(field(0), 124);
    assert_eq!(field(4), 2);
    assert_eq!(field(8) as i32, -2);
    assert_eq!(field(16), 3);
    assert_eq!(field(20), 16);
    assert_eq!(field(52), 0xff000000);
    assert_eq!(
        &dib[124..],
        &[
            3, 2, 1, 4, 30, 20, 10, 40, 140, 120, 100, 160, 220, 200, 180, 240
        ]
    );
}

#[test]
fn dib_rejects_invalid_sizes() {
    assert!(dib_v5(0, 2, &[]).is_err());
    assert!(dib_v5(u32::MAX, 1, &[]).is_err());
    assert!(dib_v5(i32::MAX as u32, i32::MAX as u32, &[]).is_err());
    assert!(dib_v5(2, 2, &[1, 2, 3, 4]).is_err());
}
