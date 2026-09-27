use super::*;

#[test]
fn data_type_kind_matches_plain_app_ordinals() {
    assert_eq!(DataType::Default.kind(), 0);
    assert_eq!(DataType::Audio.kind(), 1);
    assert_eq!(DataType::Video.kind(), 2);
    assert_eq!(DataType::Image.kind(), 3);
    assert_eq!(DataType::Sms.kind(), 4);
    assert_eq!(DataType::Contact.kind(), 5);
    assert_eq!(DataType::Note.kind(), 6);
    assert_eq!(DataType::FeedEntry.kind(), 7);
    assert_eq!(DataType::Call.kind(), 8);
    assert_eq!(DataType::Package.kind(), 21);
    assert_eq!(DataType::File.kind(), 22);
    assert_eq!(DataType::AppFile.kind(), 23);
    assert_eq!(DataType::Doc.kind(), 24);
}

#[test]
fn data_type_media_type_str_covers_media_domains_only() {
    assert_eq!(DataType::Audio.media_type_str(), Some("audio"));
    assert_eq!(DataType::Video.media_type_str(), Some("video"));
    assert_eq!(DataType::Image.media_type_str(), Some("image"));
    assert_eq!(DataType::Doc.media_type_str(), Some("doc"));
    assert_eq!(DataType::File.media_type_str(), None);
    assert_eq!(DataType::Sms.media_type_str(), None);
}
