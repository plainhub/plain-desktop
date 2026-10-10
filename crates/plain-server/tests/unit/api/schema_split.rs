//! Per-resolver-module views of the merged desktop SDL, for reviewing the
//! GraphQL surface feature by feature. `schema/schema.graphql` stays
//! authoritative; these are generated from the same `build_schema()`.
//! Each block is owned by the first feature that reaches it, so scalars,
//! directives and cross-feature types land in `common.graphql` and no
//! definition is duplicated.

use super::*;
use async_graphql::{EmptyMutation, EmptySubscription, Schema};
use std::collections::BTreeMap;
use std::path::Path;

fn split_dir() -> std::path::PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../plain-rs")).join("../schema/features")
}

#[derive(Default, async_graphql::MergedObject)]
struct ChatMutation(
    chat_message::ChatMessageMutation,
    chat_channel::ChatChannelMutation,
    chat_peer::ChatPeerMutation,
    download::DownloadMutation,
);

#[derive(Default, async_graphql::MergedObject)]
struct DiscoverMutations(discover::DiscoverMutation, pairing::PairingMutation);

fn sdl_of<Q: async_graphql::ObjectType + 'static, M: async_graphql::ObjectType + 'static>(
    query: Q,
    mutation: M,
) -> String {
    Schema::build(query, mutation, EmptySubscription)
        .finish()
        .sdl()
}

fn feature_sdl() -> Vec<(&'static str, String)> {
    vec![
        ("app", sdl_of(app::AppQuery, app::AppMutation)),
        ("app_file", sdl_of(app_file::AppFileQuery, EmptyMutation)),
        (
            "app_logs",
            sdl_of(app_logs::AppLogsQuery, app_logs::AppLogsMutation),
        ),
        ("audio", sdl_of(audio::AudioQuery, audio::AudioMutation)),
        (
            "bookmark",
            sdl_of(bookmark::BookmarkQuery, bookmark::BookmarkMutation),
        ),
        (
            "capability",
            sdl_of(capability::CapabilityQuery, capability::CapabilityMutation),
        ),
        (
            "chat",
            sdl_of(chat_query::ChatQuery, ChatMutation::default()),
        ),
        ("db", sdl_of(db::DbQuery, db::DbMutation)),
        (
            "discover",
            sdl_of(discover::DiscoverQuery, DiscoverMutations::default()),
        ),
        (
            "favorite_folder",
            sdl_of(
                favorite_folder::FavoriteFolderQuery,
                favorite_folder::FavoriteFolderMutation,
            ),
        ),
        ("feed", sdl_of(feed::FeedQuery, feed::FeedMutation)),
        ("files", sdl_of(file_query::FileInfoQuery, EmptyMutation)),
        (
            "file_upload",
            sdl_of(
                file_upload::FileUploadQuery,
                file_upload::FileUploadMutation,
            ),
        ),
        (
            "image_editor",
            sdl_of(
                image_editor_project::ImageEditorProjectQuery,
                image_editor_project::ImageEditorProjectMutation,
            ),
        ),
        (
            "media",
            sdl_of(media::MediaQueryRoot, media::MediaMutationRoot),
        ),
        ("note", sdl_of(note::NoteQuery, note::NoteMutation)),
        (
            "pomodoro",
            sdl_of(pomodoro::PomodoroQuery, pomodoro::PomodoroMutation),
        ),
        ("prefs", sdl_of(prefs::PrefsQuery, prefs::PrefsMutation)),
    ]
}

struct Block {
    name: String,
    text: String,
    shared: bool,
}

const DEFINITION_KEYWORDS: [&str; 8] = [
    "type",
    "input",
    "enum",
    "interface",
    "union",
    "scalar",
    "schema",
    "directive",
];

fn parse_blocks(sdl: &str) -> Vec<Block> {
    let lines: Vec<&str> = sdl.lines().collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut pending = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let keyword = line.split_whitespace().next().unwrap_or("");
        if !DEFINITION_KEYWORDS.contains(&keyword) {
            if !line.trim().is_empty() {
                pending.push_str(line);
                pending.push('\n');
            }
            i += 1;
            continue;
        }
        let name = if keyword == "schema" || keyword == "directive" {
            keyword.to_string()
        } else {
            line.split_whitespace().nth(1).unwrap_or("").to_string()
        };
        let mut text = std::mem::take(&mut pending);
        text.push_str(line);
        text.push('\n');
        let mut depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
        i += 1;
        while depth > 0 && i < lines.len() {
            let current = lines[i];
            text.push_str(current);
            text.push('\n');
            depth += current.matches('{').count() as i32 - current.matches('}').count() as i32;
            i += 1;
        }
        blocks.push(Block {
            name,
            text,
            shared: keyword == "directive" || keyword == "schema",
        });
    }
    blocks
}

fn render(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        out.push_str(&block.text);
        if !block.text.ends_with("\n\n") {
            out.push('\n');
        }
    }
    out
}

fn split_sdl() -> BTreeMap<String, String> {
    let features = feature_sdl();
    let mut owners: BTreeMap<String, String> = BTreeMap::new();
    for (name, sdl) in &features {
        for block in parse_blocks(sdl) {
            let owner = if block.shared { "common" } else { name };
            owners
                .entry(block.name)
                .or_insert_with(|| (*owner).to_string());
        }
    }

    let mut out = BTreeMap::new();
    for (name, sdl) in &features {
        let owned: Vec<Block> = parse_blocks(sdl)
            .into_iter()
            .filter(|block| owners.get(&block.name).map(String::as_str) == Some(*name))
            .collect();
        if !owned.is_empty() {
            out.insert(format!("{name}.graphql"), render(&owned));
        }
    }

    let mut common: Vec<Block> = Vec::new();
    let mut seen_shared: BTreeMap<String, ()> = BTreeMap::new();
    for (_, sdl) in &features {
        for block in parse_blocks(sdl) {
            if block.name == "schema"
                || owners.get(&block.name).map(String::as_str) != Some("common")
            {
                continue;
            }
            if seen_shared.insert(block.text.clone(), ()).is_none() {
                common.push(block);
            }
        }
    }
    out.insert("common.graphql".to_string(), render(&common));
    out
}

#[test]
fn split_files_match_regenerated_views() {
    let dir = split_dir();
    for (file, expected) in split_sdl() {
        let path = dir.join(file);
        let actual = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert_eq!(actual, expected, "{} is stale", path.display());
    }
}

#[test]
fn split_directory_has_no_extra_files() {
    let dir = split_dir();
    let mut actual: Vec<String> = std::fs::read_dir(&dir)
        .expect("schema/features directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".graphql"))
        .collect();
    actual.sort();
    let mut expected: Vec<String> = split_sdl().keys().cloned().collect();
    expected.sort();
    assert_eq!(actual, expected, "schema/features contents drifted");
}

#[test]
fn split_covers_every_type_of_the_merged_schema() {
    let merged = parse_blocks(&build_schema().sdl());
    let split_names: std::collections::BTreeSet<String> = feature_sdl()
        .iter()
        .flat_map(|(_, sdl)| parse_blocks(sdl))
        .filter(|block| block.name != "schema")
        .map(|block| block.name)
        .collect();
    let missing: Vec<&str> = merged
        .iter()
        .filter(|block| !block.shared)
        .filter(|block| block.name != "QueryRoot" && block.name != "MutationRoot")
        .filter(|block| !split_names.contains(&block.name))
        .map(|block| block.name.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "types absent from schema/features: {missing:?}"
    );
}

#[test]
#[ignore]
fn export_split_schema_sdl() {
    let dir = split_dir();
    std::fs::create_dir_all(&dir).expect("create schema/features directory");
    for (file, body) in split_sdl() {
        std::fs::write(dir.join(file), body).expect("write feature SDL");
    }
}
