use super::client::File;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Kind {
    File,
    Sync,
    Multi,
    Zip,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub entry: File,
    pub write_dir: String,
    pub store_to_downloads: bool,
    pub entry_name: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub targets: Vec<Target>,
    pub total_files: usize,
    pub total_size: i64,
}
struct Pending {
    entry: File,
    parent: String,
    top: bool,
    ancestors: Vec<String>,
}
pub struct Walker {
    kind: Kind,
    base: String,
    public: bool,
    pending: Vec<Pending>,
    active: Option<Pending>,
    paths: HashSet<String>,
    destinations: HashSet<String>,
    directories: usize,
    nodes: usize,
    plan: Plan,
}
fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value != "."
            && value != ".."
            && !value.chars().any(|c| matches!(c, '/' | '\\' | '\0')),
        "Invalid shared entry name"
    );
    Ok(())
}
fn join(parent: &str, child: &str) -> String {
    if child.is_empty() {
        parent.into()
    } else if parent.is_empty() {
        child.into()
    } else {
        format!("{}/{child}", parent.trim_end_matches('/'))
    }
}
impl Walker {
    pub fn new(kind: Kind, entries: Vec<File>, target: &str, downloads: &str) -> Result<Self> {
        ensure!(!entries.is_empty(), "No shared entries selected");
        ensure!(entries.len() <= 100_000, "Shared batch capacity exceeded");
        ensure!(
            !matches!(kind, Kind::File | Kind::Sync) || entries.len() == 1,
            "Expected one shared entry"
        );
        ensure!(
            kind != Kind::File || !entries[0].is_dir,
            "Expected a shared file"
        );
        ensure!(
            kind != Kind::Sync || entries[0].is_dir,
            "Expected a shared directory"
        );
        let base = if target.is_empty() { downloads } else { target };
        ensure!(
            base.starts_with('/') && !base.contains('\0'),
            "Invalid downloads directory"
        );
        let pending = entries
            .into_iter()
            .rev()
            .map(|entry| Pending {
                entry,
                parent: String::new(),
                top: true,
                ancestors: vec![],
            })
            .collect();
        Ok(Self {
            kind,
            base: if base == "/" {
                "/".into()
            } else {
                base.trim_end_matches('/').into()
            },
            public: target.is_empty(),
            pending,
            active: None,
            paths: HashSet::new(),
            destinations: HashSet::new(),
            directories: 0,
            nodes: 0,
            plan: Plan {
                targets: vec![],
                total_files: 0,
                total_size: 0,
            },
        })
    }
    pub fn next_directory(&mut self) -> Result<Option<String>> {
        ensure!(self.active.is_none(), "Shared directory reply required");
        while let Some(item) = self.pending.pop() {
            self.nodes += 1;
            ensure!(self.nodes <= 100_000, "Shared batch capacity exceeded");
            name(&item.entry.name)?;
            ensure!(item.entry.size >= 0, "Invalid shared file size");
            ensure!(
                !item.ancestors.contains(&item.entry.virtual_path),
                "Shared directory cycle"
            );
            if !self.paths.insert(item.entry.virtual_path.clone()) {
                continue;
            }
            let relative = join(&item.parent, &item.entry.name);
            if item.entry.is_dir {
                self.directories += 1;
                ensure!(
                    self.directories <= 10_000 && item.ancestors.len() < 64,
                    "Shared directory capacity exceeded"
                );
                let path = item.entry.virtual_path.clone();
                self.active = Some(item);
                return Ok(Some(path));
            }
            ensure!(
                self.destinations.insert(relative.clone()),
                "Duplicate shared destination"
            );
            self.plan.total_size = self
                .plan
                .total_size
                .checked_add(item.entry.size)
                .ok_or_else(|| anyhow::anyhow!("Shared batch size overflow"))?;
            self.plan.targets.push(Target {
                write_dir: if self.kind == Kind::Zip {
                    String::new()
                } else if self.public && item.top {
                    String::new()
                } else {
                    join(&self.base, &item.parent)
                },
                store_to_downloads: self.kind != Kind::Zip && self.public && item.top,
                entry_name: relative,
                entry: item.entry,
            });
        }
        self.plan.total_files = self.plan.targets.len();
        Ok(None)
    }
    pub fn supply(&mut self, mut children: Vec<File>) -> Result<()> {
        let item = self
            .active
            .take()
            .ok_or_else(|| anyhow::anyhow!("No shared directory requested"))?;
        ensure!(
            self.nodes + self.pending.len() + children.len() <= 100_000,
            "Shared batch capacity exceeded"
        );
        children.sort_by_key(|file| (!file.is_dir, file.name.to_lowercase()));
        let parent = join(&item.parent, &item.entry.name);
        let mut ancestors = item.ancestors;
        ancestors.push(item.entry.virtual_path);
        for entry in children.into_iter().rev() {
            self.pending.push(Pending {
                entry,
                parent: parent.clone(),
                top: false,
                ancestors: ancestors.clone(),
            });
        }
        Ok(())
    }
    pub fn finish(self) -> Result<Plan> {
        ensure!(
            self.active.is_none() && self.pending.is_empty(),
            "Shared plan incomplete"
        );
        Ok(self.plan)
    }
}
#[cfg(test)]
#[path = "../../tests/unit/shares/batch_plan.rs"]
mod tests;
