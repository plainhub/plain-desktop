use super::*;

#[derive(serde::Deserialize)]
struct Case {
    name: String,
    html: String,
    markdown: String,
}

#[test]
fn kotlin_markdown_behavior_contract() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../../testdata/html_to_markdown.json")).unwrap();
    assert!(cases.len() >= 212);
    let mut failures = Vec::new();
    for case in cases {
        let actual = html_to_markdown(&case.html);
        if actual != case.markdown {
            failures.push(format!(
                "{}: {:?}\nexpected {:?}\nactual   {:?}",
                case.name, case.html, case.markdown, actual
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn numeric_entities_use_unicode_scalars() {
    assert_eq!(html_to_markdown("&#128512; &#x1F600;"), "😀 😀");
    assert_eq!(
        html_to_markdown("&#xD800; &#1114112;"),
        "&#xD800; &#1114112;"
    );
}

#[test]
fn conversions_have_no_shared_mutable_state() {
    let workers = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..100 {
                    assert_eq!(
                        html_to_markdown("<h2>Title</h2><p>A <b>B</b>.</p>"),
                        "Title\n-----\n\nA **B**."
                    );
                    assert_eq!(html_to_markdown(""), "");
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
}
