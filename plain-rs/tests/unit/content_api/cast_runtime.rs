use super::*;
#[test]
fn callback_reads_escaped_last_change_without_advancing_metadata_duplicates() {
    let xml = r#"<e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0"><e:property><LastChange>&lt;Event&gt;&lt;InstanceID val="0"&gt;&lt;TransportState val="STOPPED"/&gt;&lt;AVTransportURIMetaData val=""/&gt;&lt;/InstanceID&gt;&lt;/Event&gt;</LastChange></e:property></e:propertyset>"#;
    let values = attributes(xml);
    assert_eq!(values["TransportState"], "STOPPED");
    assert!(values.contains_key("AVTransportURIMetaData"));
}
#[test]
fn callback_reads_cdata_position_and_pause() {
    let values = attributes(
        r#"<LastChange><![CDATA[<Event><InstanceID val="0"><TransportState val="PAUSED_PLAYBACK"/><RelTime val="01:02:03.999"/><TrackDuration val="02:00:00"/></InstanceID></Event>]]></LastChange>"#,
    );
    assert_eq!(values["TransportState"], "PAUSED_PLAYBACK");
    assert_eq!(time_ms(&values["RelTime"]), 3723000);
    assert_eq!(time_ms(&values["TrackDuration"]), 7200000);
}
#[test]
fn malformed_or_unknown_position_is_zero() {
    for value in ["", "NOT_IMPLEMENTED", "bad", "-1:00:00"] {
        assert_eq!(time_ms(value), 0);
    }
}

#[test]
fn legacy_seek_times_keep_millisecond_units_and_ignore_fractions() {
    for (text,ms) in [("00:00:00",0),("00:00:05",5000),("00:00:59",59000),("00:01:00",60000),("00:01:05",65000),("01:00:00",3600000),("01:01:01",3661000),("10:30:45",37845000),("00:00:05.999",5000)] {assert_eq!(time_ms(text),ms);}
}
