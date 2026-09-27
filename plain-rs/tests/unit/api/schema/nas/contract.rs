//! Structural contract tests — the plain-nas mirror of plain-app's
//! `ApiContractTest.kt`. Locks the conventions of `plain-app/shared/apitest/
//! API_SPEC.md` against the live SDL so new fields/operations cannot drift:
//!
//! 1. The committed snapshot (`apitest/schema.graphqls`) must match the
//!    runtime schema byte for byte — schema changes are only landed by
//!    running `print_schema` and committing the regenerated file.
//! 2. Scalar discipline (§1), naming (§2/§7), parameter discipline (§3),
//!    return shapes (§6) and the deliberate-exceptions allowlist (§8) are
//!    enforced on the SDL.
//!
//! When a test here fails after an intentional schema change: run
//! `cargo test print_schema` to regenerate the snapshot, and update the
//! allowlists below only if API_SPEC.md was updated with the same decision.
//! `QueryRoot`/`MutationRoot` are the NAS names of the phone SDL's
//! `Query`/`Mutation`.

use super::*;

// Fields that keep a raw String id although the naming rule says ID —
// the NAS-side subset of ApiContractTest.stringIdAllowlist, one-to-one
// with the "deliberate exceptions" table in API_SPEC.md §8.
const STRING_ID_ALLOWLIST: [&str; 5] = [
    "clientId", // App/Session/Event.clientId — public identity string, not an addressable entity id
    "diskId",   // StorageMount.diskId — OS disk uuid, foreign identifier
    "albumFileId", // Audio.albumFileId — album-art display fileId token
    "fileId", // chunk-flow args (uploadedChunks/mergeStatus/mergeChunks) — client-chosen chunk-set id
    "id:AppFile", // AppFile.id — content-addressable fileId (`{sha256}[.{ext}]`), same String space as ChatFiles.ids (plain-app SDL)
];

// Bulk destructive/modify mutations that must return ActionResult! (§6).
const ACTION_RESULT_MUTATIONS: [&str; 9] = [
    "deleteFiles",
    "deleteBookmarks",
    "deleteChatItems",
    "deleteMediaItems",
    "trashMediaItems",
    "restoreMediaItems",
    "moveMediaItems",
    "trashFiles",
    "restoreFiles",
];

// The subset of ACTION_RESULT_MUTATIONS addressed by `query: String!` —
// these carry the blank-query guard (`bulk_query_required`, §5); whole-table
// intent is the explicit `all:true` sentinel. deleteChatItems is the
// documented §5 exception: the chat service resolves the query to an id set
// first (resolve_chat_ids: blank → empty set → affectedCount 0, no error),
// so it must stay out of this guard list.
const BULK_QUERY_MUTATIONS: [&str; 4] = [
    "deleteMediaItems",
    "trashMediaItems",
    "restoreMediaItems",
    "moveMediaItems",
];

/// One `field(args): Type` declaration.
struct Field {
    name: String,
    args: Vec<(String, String)>,
    ty: String,
}

/// id/id-ish names, including casing drift like `diskID` (API_SPEC §1/§4).
fn is_id_name(name: &str) -> bool {
    name == "id"
        || name.ends_with("Id")
        || name.ends_with("Ids")
        || name.ends_with("IDs")
        || name.ends_with("ID")
}

/// SDL lines with `"""` description blocks removed, so doc comments can
/// mention legacy names without tripping the scanners.
fn stripped_lines(sdl: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_doc = false;
    for raw in sdl.lines() {
        let line = raw.trim();
        if line.starts_with("\"\"\"") {
            in_doc = !in_doc;
            continue;
        }
        if !in_doc {
            out.push(line);
        }
    }
    out
}

/// `(name, body_lines)` of every `type|input NAME { ... }` block.
fn blocks(sdl: &str) -> Vec<(String, Vec<&str>)> {
    let mut out: Vec<(String, Vec<&str>)> = Vec::new();
    let mut open = false;
    for line in stripped_lines(sdl) {
        let header = line
            .strip_prefix("type ")
            .or_else(|| line.strip_prefix("input "))
            .and_then(|rest| rest.strip_suffix(" {"));
        if let Some(name) = header {
            out.push((name.trim().to_string(), Vec::new()));
            open = true;
            continue;
        }
        if line == "}" {
            open = false;
            continue;
        }
        if let (true, Some((_, body))) = (open && !line.is_empty(), out.last_mut()) {
            body.push(line);
        }
    }
    out
}

fn parse_field(line: &str) -> Option<Field> {
    if let Some((head, ty)) = line.rsplit_once("): ") {
        let open = head.find('(')?;
        let args = head[open + 1..]
            .split(", ")
            .filter(|a| !a.is_empty())
            .map(|a| {
                let (n, t) = a.split_once(": ")?;
                Some((n.to_string(), t.to_string()))
            })
            .collect::<Option<Vec<_>>>()?;
        return Some(Field {
            name: head[..open].trim().to_string(),
            args,
            ty: ty.trim_end_matches(',').to_string(),
        });
    }
    let (name, ty) = line.split_once(": ")?;
    Some(Field {
        name: name.to_string(),
        args: Vec::new(),
        ty: ty.trim_end_matches(',').to_string(),
    })
}

fn fields(sdl: &str, type_name: &str) -> Vec<Field> {
    blocks(sdl)
        .into_iter()
        .find(|(name, _)| name == type_name)
        .map(|(_, body)| body.iter().filter_map(|l| parse_field(l)).collect())
        .unwrap_or_default()
}

/// The root blocks only hold operations (descriptions are stripped), so
/// every body line is one operation signature.
fn root_operations(sdl: &str, root: &str) -> Vec<Field> {
    fields(sdl, root)
}

/// Every typed field in the SDL as `(owner, field)`.
fn all_fields(sdl: &str) -> Vec<(String, Field)> {
    blocks(sdl)
        .into_iter()
        .flat_map(|(name, body)| {
            body.iter()
                .filter_map(|l| parse_field(l).map(|f| (name.clone(), f)))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn sdl_snapshot_matches_committed_file() {
    let _g = sdl_lock();
    let committed = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/nas-schema.graphqls"
    ))
    .expect("testdata/nas-schema.graphqls exists");
    let sdl = test_sdl();
    if committed != sdl {
        panic!(
            "testdata/nas-schema.graphqls is out of date with the runtime schema. \
             Run `cargo test -p plain-rs --all-features print_schema -- --ignored` and commit the regenerated file."
        );
    }
}

#[test]
fn id_suffixed_fields_use_the_id_scalar() {
    let sdl = test_sdl();
    for (owner, f) in all_fields(&sdl) {
        if !is_id_name(&f.name) {
            continue;
        }
        let uses_id = f.ty.starts_with("ID") || f.ty.starts_with("[ID");
        let owner_qualified = format!("{}:{}", f.name, owner);
        if !uses_id
            && !STRING_ID_ALLOWLIST.contains(&f.name.as_str())
            && !STRING_ID_ALLOWLIST.contains(&owner_qualified.as_str())
        {
            panic!(
                "Field {owner}.{} is typed '{}' — id-suffixed fields must use the ID scalar (API_SPEC §1/§4; §8 for exceptions).",
                f.name, f.ty
            );
        }
    }
}

#[test]
fn id_suffixed_arguments_use_the_id_scalar() {
    let sdl = test_sdl();
    for root in ["QueryRoot", "MutationRoot"] {
        for f in root_operations(&sdl, root) {
            for (arg_name, arg_ty) in &f.args {
                if !is_id_name(arg_name) {
                    continue;
                }
                let uses_id = arg_ty.starts_with("ID") || arg_ty.starts_with("[ID");
                // deleteDbTableRows(ids): raw table PKs, debug API (§8/§10).
                if !uses_id
                    && arg_name != "ids"
                    && !STRING_ID_ALLOWLIST.contains(&arg_name.as_str())
                {
                    panic!(
                        "Argument {root}.{}({arg_name}:) is typed '{arg_ty}' — id-suffixed arguments must use the ID scalar (API_SPEC §1/§4).",
                        f.name
                    );
                }
            }
        }
    }
}

#[test]
fn bulk_mutations_return_action_result() {
    let sdl = test_sdl();
    let ops = root_operations(&sdl, "MutationRoot");
    for op in ACTION_RESULT_MUTATIONS {
        let f = ops
            .iter()
            .find(|f| f.name == op)
            .unwrap_or_else(|| {
                panic!("Mutation {op} disappeared from the schema — update contract::ACTION_RESULT_MUTATIONS.")
            });
        assert_eq!(
            f.ty, "ActionResult!",
            "Bulk mutation {op} must return ActionResult! (API_SPEC §6)."
        );
    }
}

#[test]
fn bulk_query_mutations_keep_query_required() {
    let sdl = test_sdl();
    let ops = root_operations(&sdl, "MutationRoot");
    for op in BULK_QUERY_MUTATIONS {
        let f = ops.iter().find(|f| f.name == op).unwrap_or_else(|| {
            panic!(
                "Mutation {op} disappeared from the schema — update contract::BULK_QUERY_MUTATIONS."
            )
        });
        assert!(
            f.args.iter().any(|(n, t)| n == "query" && t == "String!"),
            "Mutation {op} must declare query: String! — the blank-query bulk guard relies on a required query (API_SPEC §5)."
        );
    }
}

#[test]
fn paginated_lists_pair_offset_limit_and_query() {
    let sdl = test_sdl();
    for f in root_operations(&sdl, "QueryRoot") {
        let has_offset = f.args.iter().any(|(n, _)| n == "offset");
        let has_limit = f.args.iter().any(|(n, _)| n == "limit");
        if has_offset {
            assert!(
                f.args.iter().any(|(n, t)| n == "offset" && t == "Int!"),
                "Query {} — offset must be Int! (API_SPEC §3).",
                f.name
            );
            assert!(
                has_limit && f.args.iter().any(|(n, t)| n == "limit" && t == "Int!"),
                "Query {} — paginated query must declare limit: Int! (API_SPEC §3).",
                f.name
            );
            // dbTableRows is the debug DB browser — deliberately exempt
            // (2026-09-20 user decision: debug APIs stay as-is, §10).
            assert!(
                f.args.iter().any(|(n, t)| n == "query" && t == "String!")
                    || f.name == "dbTableRows",
                "Query {} — paginated query must declare query: String! (API_SPEC §3).",
                f.name
            );
        }
        assert!(
            !has_limit || has_offset,
            "Query {} — limit without offset (API_SPEC §3).",
            f.name
        );
    }
}

#[test]
fn durations_and_sizes_carry_units_and_64_bit_width() {
    let sdl = test_sdl();
    for (owner, f) in all_fields(&sdl) {
        match f.name.as_str() {
            "duration" => {
                panic!("{owner}.duration is forbidden — use durationMs/durationSec (API_SPEC §2).")
            }
            "timeLeft" | "totalTime" | "workDuration" => panic!(
                "{owner}.{} is forbidden — the unit must be part of the name (API_SPEC §2).",
                f.name
            ),
            "size" => assert!(
                f.ty.starts_with("Long"),
                "{owner}.size is {} — sizes must be Long, GraphQL Int overflows at 2GiB (API_SPEC §1).",
                f.ty
            ),
            _ => {}
        }
        if f.name.ends_with("Bytes") {
            assert!(
                f.ty.starts_with("Long"),
                "{owner}.{} is {} — byte counts must be Long (API_SPEC §1).",
                f.name,
                f.ty
            );
        }
    }
}

#[test]
fn timestamps_are_instants_not_raw_numbers_or_strings() {
    let sdl = test_sdl();
    for (owner, f) in all_fields(&sdl) {
        if f.name.ends_with("At") || f.name == "lastActive" || f.name == "buildTime" {
            assert!(
                f.ty.starts_with("Instant"),
                "{owner}.{} is {} — time points must use the Instant scalar (API_SPEC §1).",
                f.name,
                f.ty
            );
        }
    }
}

#[test]
fn state_fields_are_enums_not_free_strings() {
    let sdl = test_sdl();
    for (owner, f) in all_fields(&sdl) {
        if f.name == "state" {
            assert_ne!(
                f.ty, "String!",
                "{owner}.state must be an enum — free-string state machines are unconsumable across platforms."
            );
        }
        if f.name == "type" {
            // Tag.type: Int is the phone-parity exception; String is not.
            assert_ne!(
                f.ty, "String!",
                "{owner}.type must be an enum — free-string type discriminators are unconsumable across platforms."
            );
        }
    }
}

#[test]
fn no_operation_is_get_prefixed() {
    let sdl = test_sdl();
    for root in ["QueryRoot", "MutationRoot"] {
        for f in root_operations(&sdl, root) {
            assert!(
                !f.name.starts_with("get"),
                "{root}.{} — GraphQL operations carry no get prefix (API_SPEC §7 naming); renamed: getTasks → fileTasks.",
                f.name
            );
        }
    }
}

#[test]
fn legacy_shapes_are_gone() {
    let sdl = stripped_lines(&test_sdl()).join("\n");
    for (pattern, why) in [
        (
            "children:",
            "renamed to childCount — File.childCount is the one true name (2026-09-24 plain-app contract)",
        ),
        (
            "filesCount",
            "renamed to fileCount (2026-09-24 plain-app contract)",
        ),
        (
            "renameAudioPlaylist",
            "renamed to updateAudioPlaylist, returns the entity (2026-09-24 plain-app contract)",
        ),
        (
            "owner: ID!",
            "renamed to ownerId (2026-09-24 plain-app contract)",
        ),
        ("getTasks", "renamed to fileTasks (no get prefix)"),
        (
            "setDeviceName(",
            "renamed to setHostname — updateDeviceName is the phone display-name op",
        ),
        (
            "MediaActionResult",
            "legacy MediaActionResult must not come back — use ActionResult",
        ),
        ("listTrash(", "renamed to trashItems (no list prefix)"),
        (
            "deleteTrash(",
            "renamed to deleteTrashItem — it deletes one trashed entry",
        ),
        (
            "pathStat(",
            "replaced by the pathExists/pathKind predicate pair",
        ),
        (
            "DeviceFeature",
            "renamed to Capability; App.features is now App.capabilities",
        ),
    ] {
        assert!(
            !sdl.contains(pattern),
            "legacy shape `{pattern}` reappeared: {why}"
        );
    }
}

#[test]
fn count_siblings_carry_the_list_filter() {
    let sdl = test_sdl();
    let queries = root_operations(&sdl, "QueryRoot");
    for f in &queries {
        if !f.args.iter().any(|(n, _)| n == "offset") {
            continue;
        }
        let count_name = format!("{}Count", f.name);
        let count_sig = queries
            .iter()
            .find(|q| q.name == count_name && !q.args.is_empty());
        if let Some(count) = count_sig {
            assert!(
                count
                    .args
                    .iter()
                    .any(|(n, t)| n == "query" && t == "String!"),
                "Query {} must declare query: String! — sibling of paginated {} (API_SPEC §3).",
                count.name,
                f.name
            );
        }
    }
}
