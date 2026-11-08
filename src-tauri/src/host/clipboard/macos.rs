use objc2::{AllocAnyThread, rc::autoreleasepool, runtime::ProtocolObject};
use objc2_app_kit::{NSImage, NSPasteboard, NSPasteboardTypeString, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

use super::Sample;

pub(super) fn read() -> Option<Sample> {
    autoreleasepool(|_| read_from(&NSPasteboard::generalPasteboard()))
}

fn read_from(pasteboard: &NSPasteboard) -> Option<Sample> {
    let change = pasteboard.changeCount();
    let items = pasteboard.pasteboardItems()?;
    // SAFETY: AppKit provides this immutable process-lifetime type constant.
    let text_type = unsafe { NSPasteboardTypeString };
    let text = items
        .iter()
        .find_map(|item| item.stringForType(text_type))?
        .to_string();
    let concealed = NSString::from_str("org.nspasteboard.ConcealedType");
    let sensitive = pasteboard
        .types()
        .is_some_and(|types| types.iter().any(|name| *name == *concealed));
    (pasteboard.changeCount() == change).then_some(Sample { text, sensitive })
}

pub(super) fn write_text(text: &str) -> Result<(), String> {
    autoreleasepool(|_| {
        let string = ProtocolObject::from_retained(NSString::from_str(text));
        write_object(&NSPasteboard::generalPasteboard(), string)
    })
}

pub(super) fn write_png(png: &[u8]) -> Result<(), String> {
    autoreleasepool(|_| write_png_to(&NSPasteboard::generalPasteboard(), png))
}

fn write_png_to(pasteboard: &NSPasteboard, png: &[u8]) -> Result<(), String> {
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(png))
        .ok_or("AppKit could not decode the clipboard image")?;
    write_object(pasteboard, ProtocolObject::from_retained(image))
}

fn write_object(
    pasteboard: &NSPasteboard,
    item: objc2::rc::Retained<ProtocolObject<dyn NSPasteboardWriting>>,
) -> Result<(), String> {
    let items = NSArray::from_retained_slice(&[item]);
    pasteboard.clearContents();
    if !pasteboard.writeObjects(&items) {
        return Err("AppKit refused the clipboard data".to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/host/clipboard_macos.rs"]
mod tests;
