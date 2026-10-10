//! Tantivy-based file search index.
//!
//! Replaces the Go mmap inverted index (`fs_index.go` + `fs_index_search.go`).
//! The index is stored on disk at `{DATA_DIR}/searchidx_files/`.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tantivy::{
    Index, IndexReader, IndexWriter, Term,
    collector::TopDocs,
    directory::MmapDirectory,
    query::{BooleanQuery, BoostQuery, Occur, Query, QueryParser, TermQuery},
    schema::{
        Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
    },
};

const INDEX_DIR_NAME: &str = "searchidx_files";

/// A single search result.
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub modified: i64,
}

/// Fields stored in the tantivy schema.
struct Fields {
    path: Field,
    path_exact: Field,
    name: Field,
    name_exact: Field,
    ext: Field,
    size: Field,
    modified: Field,
    is_dir: Field,
}

/// The file search index.
pub struct FileSearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: Fields,
}

impl FileSearchIndex {
    /// Open or create the file search index at `{data_dir}/searchidx_files/`.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let index_path = data_dir.join(INDEX_DIR_NAME);
        std::fs::create_dir_all(&index_path)?;

        let (schema, fields) = build_schema();

        let mmap_dir = MmapDirectory::open(&index_path)?;
        let index = if Index::exists(&mmap_dir)? {
            match Index::open(mmap_dir) {
                // Reuse only when the stored schema matches exactly — a
                // schema written by an older build makes the freshly-built
                // field handles point at wrong/missing segments. On drift
                // the index is recreated and rebuilt from the roots.
                Ok(existing) if existing.schema() == schema => existing,
                _ => {
                    log::info!("[file-index] stale schema at {index_path:?} — recreating");
                    std::fs::remove_dir_all(&index_path)?;
                    std::fs::create_dir_all(&index_path)?;
                    Index::create_in_dir(&index_path, schema)?
                }
            }
        } else {
            Index::create_in_dir(&index_path, schema)?
        };

        let writer = index.writer(50_000_000)?; // 50 MB heap
        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }

    /// Return true if the index directory exists and has segments.
    pub fn exists(data_dir: &Path) -> bool {
        let index_path = data_dir.join(INDEX_DIR_NAME);
        if !index_path.exists() {
            return false;
        }
        // Check for at least one segment file.
        std::fs::read_dir(&index_path)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().starts_with("meta.json"))
            })
            .unwrap_or(false)
    }

    /// Build (or rebuild) the index from the given root directories.
    pub fn build_index(&self, roots: &[PathBuf]) -> Result<usize> {
        let mut writer = self.writer.lock().unwrap();
        writer.delete_all_documents()?;
        writer.commit()?;

        let mut count = 0;
        for root in roots {
            count += self.index_directory(&mut writer, root)?;
        }
        writer.commit()?;
        self.reader.reload()?;
        Ok(count)
    }

    /// Index a single file entry.
    pub fn index_file(
        &self,
        path: &str,
        name: &str,
        is_dir: bool,
        size: i64,
        modified: i64,
    ) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let mut doc = tantivy::TantivyDocument::new();
        doc.add_text(self.fields.path, &path.replace('\\', "/"));
        doc.add_text(self.fields.path_exact, &path.replace('\\', "/"));
        doc.add_text(self.fields.name, name);
        doc.add_text(self.fields.name_exact, name);
        doc.add_text(self.fields.ext, &ext);
        doc.add_i64(self.fields.size, size);
        doc.add_i64(self.fields.modified, modified);
        doc.add_bool(self.fields.is_dir, is_dir);
        writer.add_document(doc)?;
        // Commit after each file to keep the index up-to-date for small changes.
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Remove a file from the index by path.
    pub fn remove_file(&self, path: &str) -> Result<()> {
        let normalized = path.replace('\\', "/");
        let mut writer = self.writer.lock().unwrap();
        let term = Term::from_field_text(self.fields.path_exact, &normalized);
        writer.delete_term(term);
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Search the index. Supports the Go search DSL syntax.
    pub fn search(
        &self,
        query_str: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let fields = &self.fields;

        let query_parser = QueryParser::for_index(&self.index, vec![fields.name, fields.path]);

        // Parse the query using the Go-style DSL.
        let filter_fields = crate::utils::search_dsl::parse(query_str);

        let tantivy_query: Box<dyn Query> = if filter_fields.is_empty() {
            // Match all.
            Box::new(BoostQuery::new(Box::new(tantivy::query::AllQuery), 1.0))
        } else {
            build_tantivy_query(&filter_fields, &query_parser, fields)?
        };

        let top_docs = TopDocs::with_limit(limit.max(1) + offset);
        let results = searcher.search(&tantivy_query, &top_docs)?;

        let mut out = Vec::new();
        for (_score, doc_address) in results.into_iter().skip(offset) {
            let doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;
            let path = get_str_field(&doc, fields.path_exact)
                .or_else(|| get_str_field(&doc, fields.path))
                .unwrap_or_default()
                .to_string();
            let name = get_str_field(&doc, fields.name)
                .unwrap_or_default()
                .to_string();
            let is_dir = get_bool_field(&doc, fields.is_dir).unwrap_or(false);
            let size = get_i64_field(&doc, fields.size).unwrap_or(0);
            let modified = get_i64_field(&doc, fields.modified).unwrap_or(0);

            out.push(SearchResult {
                path,
                name,
                is_dir,
                size,
                modified,
            });
        }

        Ok(out)
    }

    /// Total number of indexed documents.
    pub fn doc_count(&self) -> usize {
        self.reader.searcher().num_docs() as usize
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn index_directory(&self, writer: &mut IndexWriter, root: &Path) -> Result<usize> {
        let mut count = 0;
        for entry in crate::media::walk::Walk::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().is_dir();

            // Skip hidden files/dirs.
            if name.starts_with('.') {
                continue;
            }

            let meta = std::fs::metadata(path).ok();
            let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
            let modified = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let ext = Path::new(&name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let normalized = path.to_string_lossy().replace('\\', "/");

            let mut doc = tantivy::TantivyDocument::new();
            doc.add_text(self.fields.path, &normalized);
            doc.add_text(self.fields.path_exact, &normalized);
            doc.add_text(self.fields.name, &name);
            doc.add_text(self.fields.name_exact, &name);
            doc.add_text(self.fields.ext, &ext);
            doc.add_i64(self.fields.size, size);
            doc.add_i64(self.fields.modified, modified);
            doc.add_bool(self.fields.is_dir, is_dir);
            writer.add_document(doc)?;
            count += 1;
        }
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Schema builder
// ---------------------------------------------------------------------------

fn build_schema() -> (Schema, Fields) {
    let mut schema_builder = Schema::builder();

    let text_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    let fields = Fields {
        path: schema_builder.add_text_field("path", text_opts.clone()),
        path_exact: schema_builder.add_text_field("path_exact", STRING | STORED),
        name: schema_builder.add_text_field("name", text_opts),
        name_exact: schema_builder.add_text_field("name_exact", STRING | STORED),
        ext: schema_builder.add_text_field("ext", STRING | STORED),
        size: schema_builder.add_i64_field("size", STORED | tantivy::schema::INDEXED),
        modified: schema_builder.add_i64_field("modified", STORED | tantivy::schema::INDEXED),
        is_dir: schema_builder.add_bool_field("is_dir", STORED | tantivy::schema::INDEXED),
    };

    (schema_builder.build(), fields)
}

// ---------------------------------------------------------------------------
// Query builder (translates Go DSL FilterFields to tantivy queries)
// ---------------------------------------------------------------------------

fn build_tantivy_query(
    fields: &[crate::media::search::FilterField],
    parser: &QueryParser,
    schema: &Fields,
) -> Result<Box<dyn Query>> {
    let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    for f in fields {
        match f.name.as_str() {
            "text" => {
                // Full-text search across name + path.
                let q = parser.parse_query(&f.value)?;
                clauses.push((Occur::Must, q));
            }
            "name" => {
                let q = parser.parse_query(&format!("name:{}", f.value))?;
                clauses.push((Occur::Must, q));
            }
            "ext" => {
                let term = Term::from_field_text(schema.ext, &f.value.to_lowercase());
                let q = Box::new(TermQuery::new(term, IndexRecordOption::Basic));
                clauses.push((Occur::Must, q));
            }
            "path" => {
                let q = parser.parse_query(&format!("path:{}", f.value))?;
                clauses.push((Occur::Must, q));
            }
            "dir" => {
                if f.value == "true" {
                    let term = Term::from_field_bool(schema.is_dir, true);
                    let q = Box::new(TermQuery::new(term, IndexRecordOption::Basic));
                    clauses.push((Occur::Must, q));
                }
            }
            "size" => {
                if let Ok(bytes) = parse_size_value(&f.value) {
                    let range: Box<dyn Query> = match f.op.as_str() {
                        ">" => Box::new(tantivy::query::RangeQuery::new_i64_bounds(
                            "size".to_string(),
                            std::ops::Bound::Excluded(bytes),
                            std::ops::Bound::Unbounded,
                        )),
                        ">=" => Box::new(tantivy::query::RangeQuery::new_i64_bounds(
                            "size".to_string(),
                            std::ops::Bound::Included(bytes),
                            std::ops::Bound::Unbounded,
                        )),
                        "<" => Box::new(tantivy::query::RangeQuery::new_i64_bounds(
                            "size".to_string(),
                            std::ops::Bound::Unbounded,
                            std::ops::Bound::Excluded(bytes),
                        )),
                        "<=" => Box::new(tantivy::query::RangeQuery::new_i64_bounds(
                            "size".to_string(),
                            std::ops::Bound::Unbounded,
                            std::ops::Bound::Included(bytes),
                        )),
                        "!=" => {
                            let eq = tantivy::query::RangeQuery::new_i64_bounds(
                                "size".to_string(),
                                std::ops::Bound::Included(bytes),
                                std::ops::Bound::Included(bytes),
                            );
                            Box::new(BooleanQuery::new(vec![(Occur::MustNot, Box::new(eq))]))
                        }
                        _ => Box::new(tantivy::query::RangeQuery::new_i64_bounds(
                            "size".to_string(),
                            std::ops::Bound::Included(bytes),
                            std::ops::Bound::Included(bytes),
                        )),
                    };
                    clauses.push((Occur::Must, range));
                }
            }
            _ => {
                // Unknown field: treat as text search.
                if !f.value.is_empty() {
                    let q = parser.parse_query(&f.value)?;
                    clauses.push((Occur::Must, q));
                }
            }
        }
    }

    if clauses.is_empty() {
        Ok(Box::new(tantivy::query::AllQuery))
    } else {
        Ok(Box::new(BooleanQuery::new(clauses)))
    }
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

/// Extract a string field from a tantivy document.
fn get_str_field<'a>(doc: &'a tantivy::TantivyDocument, field: Field) -> Option<&'a str> {
    doc.get_first(field).and_then(|v| v.as_str())
}

/// Extract an i64 field from a tantivy document.
fn get_i64_field(doc: &tantivy::TantivyDocument, field: Field) -> Option<i64> {
    doc.get_first(field).and_then(|v| v.as_i64())
}

/// Extract a bool field from a tantivy document.
fn get_bool_field(doc: &tantivy::TantivyDocument, field: Field) -> Option<bool> {
    doc.get_first(field).and_then(|v| v.as_bool())
}

#[cfg(test)]
#[path = "../../tests/unit/media/index.rs"]
mod tests;
