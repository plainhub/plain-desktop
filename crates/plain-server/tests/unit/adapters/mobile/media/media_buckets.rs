use super::*;

fn fact(id: &str, name: &str, path: &str, sort_name: &str, size: i64) -> ItemFact {
    ItemFact {
        id: id.into(),
        name: name.into(),
        size,
        path: path.into(),
        sort_name: sort_name.into(),
    }
}

#[test]
fn aggregation_sums_sizes_counts_items_and_keeps_four_newest_paths() {
    let items = (0..6)
        .map(|index| fact("1", "Photos", &format!("/p/{index}"), "photos", 10))
        .collect();
    let result = aggregate("IMAGE", items).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].item_count, 6);
    assert_eq!(result[0].size, 60);
    assert_eq!(result[0].top_items, ["/p/0", "/p/1", "/p/2", "/p/3"]);
}

#[test]
fn aggregation_sorts_by_host_pinyin_key_and_rejects_invalid_facts() {
    let result = aggregate(
        "DOC",
        vec![
            fact("2", "照片", "/z/1", "zhaopian", 4),
            fact("1", "Audio", "/a/1", "audio", 3),
        ],
    )
    .unwrap();
    assert_eq!(
        result.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
        ["1", "2"]
    );
    assert!(aggregate("CONTACT", vec![]).is_err());
    assert!(aggregate("IMAGE", vec![fact("1", "Photos", "/p", "photos", -1)]).is_err());
}
