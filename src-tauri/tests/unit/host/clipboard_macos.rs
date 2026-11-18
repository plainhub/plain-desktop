use super::*;
use plain_libs::image::ImageEncoder;

#[test]
fn named_pasteboard_preserves_unicode_concealment_and_image() {
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::pasteboardWithUniqueName();
        let text = "Clipboard fixture 文档 😀";
        write_text_to(&pasteboard, text, false).unwrap();
        let sample = read_from(&pasteboard).unwrap();
        assert_eq!(sample.text, text);
        assert!(!sample.sensitive);
        write_text_to(&pasteboard, text, true).unwrap();
        assert!(read_from(&pasteboard).unwrap().sensitive);

        assert!(write_png_to(&pasteboard, b"invalid PNG").is_err());
        assert_eq!(read_from(&pasteboard).unwrap().text, text);
        let mut png = Vec::new();
        plain_libs::image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[10, 20, 30, 40], 1, 1, plain_libs::image::ExtendedColorType::Rgba8)
            .unwrap();
        write_png_to(&pasteboard, &png).unwrap();
        assert!(read_from(&pasteboard).is_none());
        assert!(NSImage::initWithPasteboard(NSImage::alloc(), &pasteboard).is_some());
        pasteboard.clearContents();
    });
}
