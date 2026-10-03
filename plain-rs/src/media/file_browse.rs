use anyhow::Result;
use std::path::Path;

use crate::media::{fsx, trash};

pub enum FilesPage {
    Entries(Vec<fsx::FileEntry>),
    Trash(Vec<trash::TrashItem>),
}

pub fn count_files(root: &str, query: &str) -> Result<i32> {
    let fields = crate::utils::search_dsl::parse(query);
    let mut root_path = root.to_string();
    let mut relative_path = String::new();
    let mut show_hidden = false;
    let mut trash_only = false;
    for field in &fields {
        match field.name.as_str() {
            "root_path" => root_path = field.value.clone(),
            "relative_path" => relative_path = field.value.clone(),
            "show_hidden" => show_hidden = field.value == "true",
            "trash" => trash_only = field.value == "true",
            _ => {}
        }
    }
    if trash_only {
        return Ok(trash::trash_count()? as i32);
    }
    let base = if relative_path.is_empty() {
        if root_path.is_empty() {
            "/".to_string()
        } else {
            root_path
        }
    } else {
        format!(
            "{}/{}",
            if root_path.is_empty() {
                "/"
            } else {
                root_path.trim_end_matches('/')
            },
            relative_path.trim_start_matches('/')
        )
    };
    let path = Path::new(&base);
    if !path.is_dir() {
        return Ok(0);
    }
    Ok(match fsx::count_dir_entries(path, show_hidden) {
        Ok(count) => count as i32,
        Err(_) => 0,
    })
}

pub async fn list_files(
    root: &str,
    offset: i32,
    limit: i32,
    query: &str,
    sort: fsx::SortBy,
) -> Result<FilesPage> {
    let fields = crate::utils::search_dsl::parse(query);
    let mut root_path = String::new();
    let mut parent = String::new();
    let mut relative_path = String::new();
    let mut text = String::new();
    let mut show_hidden = false;
    let mut trash_only = false;
    for field in &fields {
        match field.name.as_str() {
            "root_path" => root_path = field.value.clone(),
            "parent" => parent = field.value.clone(),
            "relative_path" => relative_path = field.value.clone(),
            "text" => text = field.value.clone(),
            "show_hidden" => show_hidden = field.value == "true",
            "trash" => trash_only = field.value == "true",
            _ => {}
        }
    }
    let offset = offset.max(0) as usize;
    let limit = if limit <= 0 { 1000 } else { limit as usize };
    if trash_only {
        let order = match sort {
            fsx::SortBy::DateAsc => trash::SortOrder::DeletedAtOldest,
            fsx::SortBy::DateDesc => trash::SortOrder::DeletedAtNewest,
            fsx::SortBy::NameAsc => trash::SortOrder::NameAsc,
            fsx::SortBy::NameDesc => trash::SortOrder::NameDesc,
            fsx::SortBy::SizeAsc => trash::SortOrder::SizeAsc,
            fsx::SortBy::SizeDesc => trash::SortOrder::SizeDesc,
        };
        return Ok(FilesPage::Trash(trash::list_trash(
            offset, limit, &text, order,
        )?));
    }
    let base = files_base_dir(&parent, root, &root_path, &relative_path);
    if !text.trim().is_empty() {
        let paths = crate::media::search::search_index_files(
            &text,
            &base,
            offset,
            limit,
            show_hidden,
            "",
            0,
        )?;
        let mut entries = Vec::new();
        for result in paths {
            if let Ok(entry) = fsx::stat(Path::new(&result.path)).await {
                entries.push(entry);
            }
        }
        return Ok(FilesPage::Entries(entries));
    }
    let entries = match sort {
        fsx::SortBy::NameAsc | fsx::SortBy::NameDesc => {
            fsx::list_dir_paged(Path::new(&base), show_hidden, offset, limit, sort).await?
        }
        _ => {
            let mut entries = fsx::list_dir(Path::new(&base), show_hidden).await?;
            match sort {
                fsx::SortBy::DateAsc => entries.sort_by(|a, b| {
                    match a.is_dir.cmp(&b.is_dir).reverse() {
                        std::cmp::Ordering::Equal => {}
                        order => return order,
                    }
                    a.updated_at.cmp(&b.updated_at)
                }),
                fsx::SortBy::DateDesc => entries.sort_by(|a, b| {
                    match a.is_dir.cmp(&b.is_dir).reverse() {
                        std::cmp::Ordering::Equal => {}
                        order => return order,
                    }
                    b.updated_at.cmp(&a.updated_at)
                }),
                fsx::SortBy::SizeAsc => entries.sort_by(|a, b| {
                    match a.is_dir.cmp(&b.is_dir).reverse() {
                        std::cmp::Ordering::Equal => {}
                        order => return order,
                    }
                    a.size.cmp(&b.size)
                }),
                fsx::SortBy::SizeDesc => entries.sort_by(|a, b| {
                    match a.is_dir.cmp(&b.is_dir).reverse() {
                        std::cmp::Ordering::Equal => {}
                        order => return order,
                    }
                    b.size.cmp(&a.size)
                }),
                _ => {}
            }
            if offset >= entries.len() {
                return Ok(FilesPage::Entries(Vec::new()));
            }
            let end = (offset + limit).min(entries.len());
            entries[offset..end].to_vec()
        }
    };
    Ok(FilesPage::Entries(entries))
}

fn files_base_dir(parent: &str, root: &str, root_path: &str, relative_path: &str) -> String {
    let base = if !parent.is_empty() {
        parent
    } else if !root.is_empty() {
        root
    } else {
        root_path
    };
    if relative_path.is_empty() {
        if base.is_empty() {
            "/".to_string()
        } else {
            base.to_string()
        }
    } else {
        format!(
            "{}/{}",
            if base.is_empty() {
                "/"
            } else {
                base.trim_end_matches('/')
            },
            relative_path.trim_start_matches('/')
        )
    }
}
