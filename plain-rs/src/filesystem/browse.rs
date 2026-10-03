use super::{SortBy, record::FileRecord};
use crate::utils::search_dsl::{FilterField, parse};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path, time::SystemTime};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub root: String,
    pub query: String,
    pub text: Option<String>,
    pub show_hidden: Option<bool>,
    pub sort_by: SortBy,
    pub offset: usize,
    pub limit: Option<usize>,
    pub count_only: bool,
}
pub struct Plan {
    pub root: String,
    show_hidden: bool,
    text: String,
    sizes: Vec<FilterField>,
    recursive: bool,
    sort: SortBy,
    offset: usize,
    limit: Option<usize>,
    count_only: bool,
}
#[derive(Serialize)]
pub struct Page {
    pub count: usize,
    pub items: Vec<FileRecord>,
}
impl Request {
    pub fn plan(self) -> io::Result<Plan> {
        let fields = parse(&self.query);
        let first = |name: &str| {
            fields
                .iter()
                .find(|f| f.name == name)
                .map(|f| f.value.clone())
                .unwrap_or_default()
        };
        let parent = first("parent");
        let root = if parent.is_empty() { self.root } else { parent };
        if !Path::new(&root).is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute directory path required",
            ));
        }
        let sizes: Vec<_> = fields
            .iter()
            .filter(|f| f.name == "file_size")
            .cloned()
            .collect();
        let recursive = self.text.is_some() || !first("text").is_empty() || !sizes.is_empty();
        Ok(Plan {
            root,
            show_hidden: self
                .show_hidden
                .unwrap_or(first("show_hidden").eq_ignore_ascii_case("true")),
            text: self.text.unwrap_or(first("text")).to_lowercase(),
            sizes,
            recursive,
            sort: self.sort_by,
            offset: self.offset,
            limit: self.limit,
            count_only: self.count_only,
        })
    }
}
impl Plan {
    pub fn archive_path(&self) -> Option<&str> {
        self.root.split_once("!zip!/").map(|(path, _)| path)
    }
    fn matches(&self, item: &FileRecord, archive: bool) -> bool {
        (archive || item.name.to_lowercase().contains(&self.text))
            && self.sizes.iter().all(|field| {
                if item.is_dir {
                    return false;
                }
                let Ok(value) = field.value.trim().parse::<i64>() else {
                    return false;
                };
                match field.op.as_str() {
                    ">" => item.size > value,
                    ">=" => item.size >= value,
                    "<" => item.size < value,
                    "<=" => item.size <= value,
                    "!=" => item.size != value,
                    "=" | ":" | "" => item.size == value,
                    _ => true,
                }
            })
    }
    pub fn archive_page(&self, mut items: Vec<FileRecord>) -> io::Result<Page> {
        let prefix = format!("{}/", self.root.trim_end_matches('/'));
        if items.iter().any(|item| {
            item.name.is_empty()
                || item.name == "."
                || item.name == ".."
                || item.name.contains('/')
                || item
                    .path
                    .strip_prefix(&prefix)
                    .map(|relative| relative.trim_end_matches('/') != item.name)
                    .unwrap_or(true)
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid archive entry receipt",
            ));
        }
        items.retain(|item| self.matches(item, true));
        Ok(self.page(items))
    }
    fn page(&self, mut items: Vec<FileRecord>) -> Page {
        let count = items.len();
        if self.count_only {
            return Page {
                count,
                items: Vec::new(),
            };
        }
        items.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                match self.sort {
                    SortBy::NameAsc => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortBy::NameDesc => b.name.to_lowercase().cmp(&a.name.to_lowercase()),
                    SortBy::DateAsc => a.updated_at.cmp(&b.updated_at),
                    SortBy::DateDesc => b.updated_at.cmp(&a.updated_at),
                    SortBy::SizeAsc => a.size.cmp(&b.size),
                    SortBy::SizeDesc => b.size.cmp(&a.size),
                }
                .then_with(|| a.path.cmp(&b.path))
            })
        });
        Page {
            count,
            items: items
                .into_iter()
                .skip(self.offset)
                .take(self.limit.unwrap_or(usize::MAX))
                .collect(),
        }
    }
    pub fn execute(&self) -> io::Result<Page> {
        let root = Path::new(&self.root);
        match fs::metadata(root) {
            Ok(meta) if !meta.is_dir() => return Ok(self.page(Vec::new())),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(self.page(Vec::new())),
            Err(e) => return Err(e),
        }
        let mut directories = vec![fs::read_dir(root)?];
        let mut items = Vec::new();
        let mut count = 0usize;
        while let Some(directory) = directories.last_mut() {
            let Some(entry) = directory.next() else {
                directories.pop();
                continue;
            };
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if self.recursive && meta.is_dir() {
                match fs::read_dir(&path) {
                    Ok(directory) => directories.push(directory),
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
                        ) => {}
                    Err(e) => return Err(e),
                }
            }
            let record = FileRecord {
                name,
                path: path.to_string_lossy().into_owned(),
                permission: String::new(),
                created_at: meta.created().ok().map(millis),
                updated_at: millis(meta.modified()?),
                size: if meta.is_dir() {
                    0
                } else {
                    i64::try_from(meta.len()).map_err(io::Error::other)?
                },
                is_dir: meta.is_dir(),
                child_count: 0,
                media_id: String::new(),
            };
            if self.matches(&record, false) {
                count += 1;
                if !self.count_only {
                    items.push(record);
                }
            }
        }
        if self.count_only {
            return Ok(Page {
                count,
                items: Vec::new(),
            });
        }
        let mut page = self.page(items);
        for item in &mut page.items {
            if item.is_dir {
                item.child_count =
                    match super::count_dir_entries(Path::new(&item.path), self.show_hidden) {
                        Ok(count) => i32::try_from(count).unwrap_or(i32::MAX),
                        Err(e)
                            if matches!(
                                e.kind(),
                                io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
                            ) =>
                        {
                            0
                        }
                        Err(e) => return Err(e),
                    };
            }
        }
        Ok(page)
    }
}
fn millis(time: SystemTime) -> i64 {
    chrono::DateTime::<chrono::Utc>::from(time).timestamp_millis()
}

#[cfg(test)]
#[path = "../../tests/unit/filesystem/browse.rs"]
mod tests;
