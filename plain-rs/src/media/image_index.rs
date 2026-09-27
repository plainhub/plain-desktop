//! Tantivy-based media search index.
//!
//! Replaces the Go media mmap index (`search_index.go`). The index is
//! stored on disk at `{DATA_DIR}/searchidx_media/`. The KV media index
//! (`media:uuid:` rows) is the source of truth; this index is derived and
//! kept in sync by the scan pipeline (`media_scan::scan_tree`), the
//! watcher and the delete paths.

use anyhow::Result;
use std::ops::Bound;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use tantivy::{
    Index, IndexReader, IndexWriter, Searcher, Term,
    collector::Count,
    directory::MmapDirectory,
    query::{AllQuery, BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery},
    schema::{
        Field, IndexRecordOption, STORED, STRING, Schema, TEXT, TextFieldIndexing, TextOptions,
        Value,
    },
};

use crate::media::scan::MediaFile;

const INDEX_DIR_NAME: &str = "searchidx_media";

/// A media search result.
#[derive(Clone, Debug)]
pub struct MediaSearchResult {
    pub uuid: String,
    pub path: String,
    pub name: String,
    pub media_type: String,
    pub size: i64,
    pub modified: i64,
    pub duration_secs: u32,
    pub artist: String,
    pub title: String,
    pub is_trash: bool,
}

/// Sort orders accepted by [`MediaSearchIndex::search`]. Mirrors the GraphQL
/// `FileSortBy` enum. NAME_* cannot be answered by tantivy's fast-field
/// ordering, so those fall back to an in-memory sort of the (capped) match
/// set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaSort {
    DateAsc,
    DateDesc,
    SizeAsc,
    SizeDesc,
    NameAsc,
    NameDesc,
}

struct Fields {
    uuid: Field,
    name: Field,
    path: Field,
    /// Directory components of the path (tokenized), for `excluded_dir:` filters.
    dir: Field,
    /// Exact parent directory (one term per doc), for per-bucket queries.
    parent: Field,
    media_type: Field,
    /// Lowercased filename extension. Only doc-kind files carry it: the
    /// term dictionary of this field then IS the docExtGroups aggregation,
    /// and it backs the `ext:` search filter.
    ext: Field,
    size: Field,
    modified: Field,
    duration_secs: Field,
    artist: Field,
    title: Field,
    is_trash: Field,
}

/// The media search index.
pub struct MediaSearchIndex {
    index: Index,
    schema: Schema,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: Fields,
}

impl MediaSearchIndex {
    /// Open or create the media search index. An index directory written by
    /// an older schema is recreated from scratch (the KV rows are the source
    /// of truth; the next scan or startup rebuild repopulates it).
    pub fn open(data_dir: &Path) -> Result<Self> {
        let index_path = data_dir.join(INDEX_DIR_NAME);
        std::fs::create_dir_all(&index_path)?;

        let (schema, fields) = build_schema();

        let mmap_dir = MmapDirectory::open(&index_path)?;
        let index = if Index::exists(&mmap_dir)? {
            match Index::open(MmapDirectory::open(&index_path)?) {
                // Only an index with the exact current schema is reused.
                // Field-presence probes are not enough: an interim layout
                // (same fields, `size`/`modified` missing the FAST markers)
                // passes them and then fails every sorted query with
                // "Field … is not a fast field". Any drift recreates the
                // index; startup healing repopulates it from the KV rows.
                Ok(existing) if existing.schema() == schema => existing,
                _ => {
                    log::info!("[media-index] stale schema at {index_path:?} — recreating");
                    std::fs::remove_dir_all(&index_path)?;
                    std::fs::create_dir_all(&index_path)?;
                    Index::create_in_dir(&index_path, schema)?
                }
            }
        } else {
            Index::create_in_dir(&index_path, schema)?
        };

        let writer = index.writer(50_000_000)?;
        let reader = index.reader()?;

        Ok(Self {
            schema: index.schema(),
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }

    /// Return true if the index exists.
    pub fn exists(data_dir: &Path) -> bool {
        let index_path = data_dir.join(INDEX_DIR_NAME);
        if !index_path.exists() {
            return false;
        }
        std::fs::read_dir(&index_path)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().starts_with("meta.json"))
            })
            .unwrap_or(false)
    }

    /// Build the media index from all MediaFile entries in the KV store.
    pub fn build_from_db(&self, db: &crate::media::kv::Db) -> Result<usize> {
        {
            let mut writer = self.writer.lock().unwrap();
            writer.delete_all_documents()?;
            writer.commit()?;
        }

        let mut count = 0;
        for kv in db.scan_prefix(b"media:uuid:") {
            let (_k, v) = kv?;
            let mf: MediaFile = match serde_json::from_slice(&v) {
                Ok(m) => m,
                Err(_) => continue,
            };
            self.add_media_file(&mf)?;
            count += 1;
        }

        self.commit()?;
        Ok(count)
    }

    /// Stage one media file into the writer. Callers driving bulk pipelines
    /// call this repeatedly and [`MediaSearchIndex::commit`] at batch
    /// boundaries; single-file callers can use [`Self::index_media_file`].
    pub fn add_media_file(&self, mf: &MediaFile) -> Result<()> {
        let writer = self.writer.lock().unwrap();
        // Remove old entry first (upsert semantics for watcher re-sends).
        writer.delete_term(Term::from_field_text(self.fields.uuid, &mf.uuid));
        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(self.fields.uuid, &mf.uuid);
        doc.add_text(self.fields.name, &mf.name);
        doc.add_text(self.fields.path, &mf.path.replace('\\', "/"));
        for comp in dir_components(&mf.path) {
            doc.add_text(self.fields.dir, comp);
        }
        // Same string the KV bucket counters key on (raw path, no
        // backslash normalization) so per-bucket queries hit exactly.
        doc.add_text(self.fields.parent, parent_dir_of(&mf.path));
        doc.add_text(self.fields.media_type, &mf.r#type);
        // Only doc-kind files carry an ext term, so this field's term
        // dictionary is exactly the candidate set for `docExtGroups`.
        if mf.r#type == "doc" {
            let ext = crate::media::scan::ext_of(&mf.name);
            if !ext.is_empty() {
                doc.add_text(self.fields.ext, ext);
            }
        }
        doc.add_i64(self.fields.size, mf.size);
        doc.add_i64(self.fields.modified, mf.modified_at);
        doc.add_u64(self.fields.duration_secs, mf.duration_sec as u64);
        doc.add_text(self.fields.artist, &mf.artist);
        doc.add_text(self.fields.title, &mf.title);
        doc.add_bool(self.fields.is_trash, mf.is_trash);
        writer.add_document(doc)?;
        Ok(())
    }

    /// Index a single media file and commit (watcher / upload paths).
    pub fn index_media_file(&self, mf: &MediaFile) -> Result<()> {
        self.add_media_file(mf)?;
        self.commit()
    }

    /// Flush staged documents so searchers see them.
    pub fn commit(&self) -> Result<()> {
        {
            let mut writer = self.writer.lock().unwrap();
            writer.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    /// Remove media files from the index by uuid and commit.
    pub fn remove_by_uuids(&self, uuids: &[&str]) -> Result<()> {
        if uuids.is_empty() {
            return Ok(());
        }
        {
            let mut writer = self.writer.lock().unwrap();
            for u in uuids {
                writer.delete_term(Term::from_field_text(self.fields.uuid, u));
            }
            writer.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    /// Remove a media file from the index by uuid.
    pub fn remove_by_uuid(&self, uuid: &str) -> Result<()> {
        self.remove_by_uuids(&[uuid])
    }

    /// Drop every document (used by `reset_all` before a full rescan).
    pub fn clear(&self) -> Result<()> {
        {
            let mut writer = self.writer.lock().unwrap();
            writer.delete_all_documents()?;
            writer.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    /// Search the media index.
    ///
    /// `query_str` uses the app's search DSL (`trash:true`, `excluded_dir:x`,
    /// `size:>10MB`, bare text, …) parsed by [`crate::utils::search_dsl::parse`].
    pub fn search(
        &self,
        query_str: &str,
        media_type: Option<&str>,
        is_trash: Option<bool>,
        sort: MediaSort,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MediaSearchResult>> {
        let searcher = self.reader.searcher();
        let query = self.build_query(query_str, media_type, is_trash)?;

        // NAME_* needs a global sort; fetch the match set (capped) and sort
        // in memory. Everything else uses the u64 fast fields directly.
        if matches!(sort, MediaSort::NameAsc | MediaSort::NameDesc) {
            // Beyond this many matches a name sort degrades to the capped
            // prefix — a pathological request for multi-100k libraries.
            const NAME_SORT_CAP: usize = 100_000;
            let top = searcher.search(
                &query,
                &tantivy::collector::TopDocs::with_limit(NAME_SORT_CAP),
            )?;
            let mut rows = Vec::with_capacity(top.len());
            for (_score, addr) in top {
                rows.push(self.doc_to_result(&searcher, addr)?);
            }
            rows.sort_by(|a, b| {
                let ord = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                if sort == MediaSort::NameAsc {
                    ord
                } else {
                    ord.reverse()
                }
            });
            return Ok(rows.into_iter().skip(offset).take(limit).collect());
        }

        let (order_field, order) = match sort {
            MediaSort::DateDesc => ("modified", tantivy::Order::Desc),
            MediaSort::DateAsc => ("modified", tantivy::Order::Asc),
            MediaSort::SizeDesc => ("size", tantivy::Order::Desc),
            MediaSort::SizeAsc => ("size", tantivy::Order::Asc),
            MediaSort::NameAsc | MediaSort::NameDesc => unreachable!(),
        };
        let collector = tantivy::collector::TopDocs::with_limit(limit.max(1) + offset)
            .order_by_fast_field::<i64>(order_field, order);
        let top = searcher.search(&query, &collector)?;

        let mut out = Vec::with_capacity(top.len());
        for (_sort_key, doc_address) in top.into_iter().skip(offset) {
            out.push(self.doc_to_result(&searcher, doc_address)?);
        }
        Ok(out)
    }

    /// Count results matching the given filters. Uses tantivy's counting
    /// collector — no document fetches.
    pub fn count(
        &self,
        query_str: &str,
        media_type: Option<&str>,
        is_trash: Option<bool>,
    ) -> Result<usize> {
        let searcher = self.reader.searcher();
        let query = self.build_query(query_str, media_type, is_trash)?;
        Ok(searcher.search(&query, &Count)?)
    }

    /// Total number of indexed documents.
    pub fn doc_count(&self) -> usize {
        self.reader.searcher().num_docs() as usize
    }

    /// `docExtGroups`: (ext, count) for every extension present on doc-kind
    /// files, sorted by ext ascending. Counts are plain-app semantics — no
    /// trash filter, matching `docCount("")`.
    ///
    /// Candidates come from the `ext` field's term dictionaries (only doc
    /// rows carry an ext term), so the cost is one dictionary stream per
    /// segment plus one counting-collector query per *distinct* extension —
    /// independent of library size and of any deleted documents left in
    /// segments by upserts. No document fetches.
    pub fn doc_ext_groups(&self) -> Result<Vec<(String, i64)>> {
        let searcher = self.reader.searcher();
        let mut exts: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for seg in searcher.segment_readers() {
            let inv = seg.inverted_index(self.fields.ext)?;
            let mut stream = inv.terms().stream()?;
            while stream.advance() {
                if let Ok(s) = std::str::from_utf8(stream.key()) {
                    exts.insert(s.to_string());
                }
            }
        }
        let mut out = Vec::with_capacity(exts.len());
        for ext in exts {
            // Count through the normal query path so deletion bitsets and
            // the doc-kind filter apply — dictionary freqs alone would
            // include docs removed by upserts.
            let n = self.count(&format!("ext:{}", ext), Some("doc"), None)?;
            if n > 0 {
                out.push((ext, n as i64));
            }
        }
        Ok(out)
    }

    /// For each wanted directory, its up-to-`n` most recent non-trash paths
    /// of `media_type` (bucket cover thumbnails). One index query per
    /// directory — `parent` term + type + non-trash, top-`n` by `modified` —
    /// so cost tracks the buckets' own documents, never the whole library,
    /// and only the returned docs are fetched from storage.
    pub fn bucket_top_items(
        &self,
        media_type: &str,
        wanted: &std::collections::HashSet<String>,
        n: usize,
    ) -> Result<std::collections::HashMap<String, Vec<String>>> {
        let mut out: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::with_capacity(wanted.len());
        if wanted.is_empty() || n == 0 {
            return Ok(out);
        }

        let searcher = self.reader.searcher();
        let collector = tantivy::collector::TopDocs::with_limit(n)
            .order_by_fast_field::<i64>("modified", tantivy::Order::Desc);
        for dir in wanted {
            let query = BooleanQuery::new(vec![
                (
                    Occur::Must,
                    term_query(Term::from_field_text(self.fields.parent, dir)),
                ),
                (
                    Occur::Must,
                    term_query(Term::from_field_text(self.fields.media_type, media_type)),
                ),
                (
                    Occur::Must,
                    term_query(Term::from_field_bool(self.fields.is_trash, false)),
                ),
            ]);
            let top = searcher.search(&query, &collector)?;
            let mut paths = Vec::with_capacity(top.len());
            for (_modified, addr) in top {
                let doc: tantivy::TantivyDocument = searcher.doc(addr)?;
                if let Some(p) = doc
                    .get_first(self.fields.path)
                    .and_then(|v| v.as_str())
                    .filter(|p| !p.is_empty())
                {
                    paths.push(p.to_string());
                }
            }
            out.insert(dir.clone(), paths);
        }
        Ok(out)
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Translate the app search DSL + filters into a tantivy query.
    fn build_query(
        &self,
        query_str: &str,
        media_type: Option<&str>,
        is_trash: Option<bool>,
    ) -> Result<Box<dyn Query>> {
        let fields = &self.fields;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        if let Some(mt) = media_type {
            if !mt.is_empty() {
                let term = Term::from_field_text(fields.media_type, mt);
                clauses.push((Occur::Must, term_query(term)));
            }
        }
        if let Some(trash) = is_trash {
            let term = Term::from_field_bool(fields.is_trash, trash);
            clauses.push((Occur::Must, term_query(term)));
        }

        // Full-text parser over the free-text fields (name/path/artist/title).
        let text_parser = QueryParser::for_index(
            &self.index,
            vec![fields.name, fields.path, fields.artist, fields.title],
        );
        // Dir-component parser for excluded_dir filters.
        let dir_parser = QueryParser::for_index(&self.index, vec![fields.dir]);

        for f in crate::utils::search_dsl::parse(query_str) {
            match f.name.as_str() {
                "trash" => {
                    let t = f.value.eq_ignore_ascii_case("true");
                    let term = Term::from_field_bool(fields.is_trash, t);
                    clauses.push((must_occur(&f.op), term_query(term)));
                }
                "excluded_dir" => {
                    // Exclude any document whose directory path contains this
                    // component (children of the excluded dir inherit it).
                    let escaped = f.value.replace('"', "");
                    if let Ok(q) = dir_parser.parse_query(&format!("\"{escaped}\"")) {
                        clauses.push((Occur::MustNot, Box::new(q)));
                    }
                }
                "size" => {
                    if let Some((op, bytes)) = parse_size_op(&f.op, &f.value) {
                        let name = self.schema.get_field_name(fields.size).to_string();
                        clauses.push((must_occur(&f.op), size_range(&name, bytes, op)));
                    }
                }
                "name" => {
                    if let Ok(q) = text_parser.parse_query(&format!("name:{}", quote(&f.value))) {
                        clauses.push((must_occur(&f.op), Box::new(q)));
                    }
                }
                "ext" => {
                    // Exact extension match (lowercased at index and query
                    // time — SQLite LIKE-style insensitivity, plain-app
                    // parity). Extensionless docs have no term and match
                    // nothing, which mirrors Android's `%.ext` LIKE.
                    let ext = f.value.to_lowercase();
                    if !ext.is_empty() {
                        let term = Term::from_field_text(fields.ext, &ext);
                        clauses.push((must_occur(&f.op), term_query(term)));
                    }
                }
                "path" => {
                    if let Ok(q) = text_parser.parse_query(&format!("path:{}", quote(&f.value))) {
                        clauses.push((must_occur(&f.op), Box::new(q)));
                    }
                }
                "bucket_id" => {
                    // A media bucket IS its containing directory (the id
                    // `mediaBuckets` reports, `bucket_of` on rows) — exact
                    // term on `parent`, filesystem root "/" included.
                    clauses.push((
                        must_occur(&f.op),
                        term_query(Term::from_field_text(fields.parent, &f.value)),
                    ));
                }
                _ => {
                    // `text` and unknown fields go through the all-fields parser.
                    if !f.value.is_empty() {
                        if let Ok(q) = text_parser.parse_query(&quote(&f.value)) {
                            clauses.push((must_occur(&f.op), Box::new(q)));
                        }
                    }
                }
            }
        }

        Ok(if clauses.is_empty() {
            Box::new(AllQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        })
    }

    fn doc_to_result(
        &self,
        searcher: &Searcher,
        addr: tantivy::DocAddress,
    ) -> Result<MediaSearchResult> {
        let doc: tantivy::TantivyDocument = searcher.doc(addr)?;
        let fields = &self.fields;
        let get_str = |f: Field| {
            doc.get_first(f)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        Ok(MediaSearchResult {
            uuid: get_str(fields.uuid),
            path: get_str(fields.path),
            name: get_str(fields.name),
            media_type: get_str(fields.media_type),
            size: doc
                .get_first(fields.size)
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            modified: doc
                .get_first(fields.modified)
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            duration_secs: doc
                .get_first(fields.duration_secs)
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            artist: get_str(fields.artist),
            title: get_str(fields.title),
            is_trash: doc
                .get_first(fields.is_trash)
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }
}

// ---------------------------------------------------------------------------
// Global instance
// ---------------------------------------------------------------------------

static MEDIA_INDEX: OnceLock<Arc<MediaSearchIndex>> = OnceLock::new();

/// Process-wide media search index rooted at `AppPaths::detect().data_dir`.
pub fn global() -> Arc<MediaSearchIndex> {
    MEDIA_INDEX
        .get_or_init(|| {
            let data_dir = crate::media::paths::detect().data_dir;
            match MediaSearchIndex::open(&data_dir) {
                Ok(idx) => Arc::new(idx),
                Err(e) => {
                    // Last resort so a broken index dir cannot take the API
                    // down: an anonymous temp dir keeps resolvers answering
                    // (empty). The open error is loud in the log.
                    log::error!(
                        "[media-index] open {data_dir:?} failed: {e} — using temp fallback"
                    );
                    let tmp = std::env::temp_dir().join(format!(
                        "plainnas-media-idx-fallback-{}",
                        std::process::id()
                    ));
                    let _ = std::fs::remove_dir_all(&tmp);
                    Arc::new(MediaSearchIndex::open(&tmp).expect("fallback media index"))
                }
            }
        })
        .clone()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn must_occur(op: &str) -> Occur {
    if op.starts_with('!') {
        Occur::MustNot
    } else {
        Occur::Must
    }
}

fn term_query(term: Term) -> Box<dyn Query> {
    Box::new(TermQuery::new(term, IndexRecordOption::Basic))
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', " "))
}

/// Split a path into lowercased directory components (for the `dir` field).
fn dir_components(path: &str) -> Vec<String> {
    Path::new(path)
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| c.as_os_str().to_str())
                .map(|s| s.to_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

/// `op`+`value` → (normalized op, byte count) for size filters.
fn parse_size_op(op: &str, value: &str) -> Option<(&'static str, i64)> {
    let bytes = parse_size_value(value).ok()?;
    let op = match op {
        ">" => ">",
        ">=" => ">=",
        "<" => "<",
        "<=" => "<=",
        "=" | "" => "=",
        "!=" => "!=",
        _ => return None,
    };
    Some((op, bytes))
}

fn size_range(field_name: &str, bytes: i64, op: &str) -> Box<dyn Query> {
    let (lo, hi) = match op {
        ">" => (Bound::Excluded(bytes), Bound::Unbounded),
        ">=" => (Bound::Included(bytes), Bound::Unbounded),
        "<" => (Bound::Unbounded, Bound::Excluded(bytes)),
        "<=" => (Bound::Unbounded, Bound::Included(bytes)),
        "!=" => {
            let eq = Box::new(RangeQuery::new_i64_bounds(
                field_name.to_string(),
                Bound::Included(bytes),
                Bound::Included(bytes),
            )) as Box<dyn Query>;
            return Box::new(BooleanQuery::new(vec![(Occur::MustNot, eq)]));
        }
        _ => (Bound::Included(bytes), Bound::Included(bytes)),
    };
    Box::new(RangeQuery::new_i64_bounds(field_name.to_string(), lo, hi))
}

/// Parse a size value like "1024", "10KB", "1MB", "2GB".
fn parse_size_value(s: &str) -> Result<i64> {
    let s = s.trim();
    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("GB") {
        (n, 1_073_741_824i64)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1_048_576i64)
    } else if let Some(n) = s.strip_suffix("KB") {
        (n, 1_024i64)
    } else if let Some(n) = s.strip_suffix('G') {
        (n, 1_073_741_824i64)
    } else if let Some(n) = s.strip_suffix('M') {
        (n, 1_048_576i64)
    } else if let Some(n) = s.strip_suffix('K') {
        (n, 1_024i64)
    } else {
        (s, 1i64)
    };
    let num: i64 = num_str.trim().parse()?;
    Ok(num * multiplier)
}

fn build_schema() -> (Schema, Fields) {
    let mut schema_builder = Schema::builder();

    let text_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    let fields = Fields {
        uuid: schema_builder.add_text_field("uuid", STRING | STORED),
        // name/path are returned in search results, so they must be STORED
        // (tantivy's TEXT constant is index-only in 0.22!).
        name: schema_builder.add_text_field("name", TEXT | STORED),
        path: schema_builder.add_text_field("path", TEXT | STORED),
        // dir is only ever matched (excluded_dir), never read back.
        dir: schema_builder.add_text_field("dir", text_opts),
        // Exact parent dir as a single term: per-bucket top-N queries.
        parent: schema_builder.add_text_field("parent", STRING),
        media_type: schema_builder.add_text_field("media_type", STRING | STORED),
        // Exact-match extension term (docs only): docExtGroups candidates +
        // `ext:` filter. Never read back, so not STORED.
        ext: schema_builder.add_text_field("ext", STRING),
        // FAST = sortable via TopDocs::order_by_fast_field.
        size: schema_builder.add_i64_field(
            "size",
            STORED | tantivy::schema::INDEXED | tantivy::schema::FAST,
        ),
        modified: schema_builder.add_i64_field(
            "modified",
            STORED | tantivy::schema::INDEXED | tantivy::schema::FAST,
        ),
        duration_secs: schema_builder.add_u64_field("duration", STORED | tantivy::schema::INDEXED),
        artist: schema_builder.add_text_field("artist", TEXT | STORED),
        title: schema_builder.add_text_field("title", TEXT | STORED),
        is_trash: schema_builder.add_bool_field("is_trash", STORED | tantivy::schema::INDEXED),
    };

    (schema_builder.build(), fields)
}

// ---------------------------------------------------------------------------
// KV bucket index: `media:bucket:{type}:{dir}` → decimal count.
// Maintained by the scan pipeline in the same staged batches as the media
// rows, so `mediaBuckets` needs no full-library pass.
// ---------------------------------------------------------------------------

/// Key prefix of the per-directory bucket counters.
pub const BUCKET_PREFIX: &str = "media:bucket:";

/// Types that participate in bucket grouping.
pub fn bucketed_type(t: &str) -> bool {
    matches!(t, "audio" | "video" | "image" | "doc")
}

/// The bucket counter key for a media row's directory.
pub fn bucket_key(media_type: &str, dir: &str) -> String {
    format!("{BUCKET_PREFIX}{media_type}:{dir}")
}

/// Parent-directory string of a media path ("" for filesystem root).
pub fn parent_dir_of(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "../../tests/unit/media/image_index.rs"]
mod tests;
