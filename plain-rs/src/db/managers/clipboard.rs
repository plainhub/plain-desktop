use crate::db::{ClipboardRow, Db};
use rusqlite::{OptionalExtension, params, params_from_iter, types::Value};

const CLIPBOARD_COLUMNS: &str = "id, text, hash, source, label, sensitive, created_at";

fn clipboard_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardRow> {
    Ok(ClipboardRow {
        id: row.get(0)?,
        text: row.get(1)?,
        hash: row.get(2)?,
        source: row.get(3)?,
        label: row.get(4)?,
        sensitive: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
    })
}

impl Db {
    pub fn clipboard_record(&self, row: &ClipboardRow) -> rusqlite::Result<(ClipboardRow, bool)> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let inserted = tx.execute(
                "INSERT INTO clipboards (id,text,hash,source,label,sensitive,created_at)
                 SELECT ?1,?2,?3,?4,?5,?6,?7
                 WHERE NOT EXISTS (SELECT 1 FROM clipboards WHERE hash=?3)",
                params![row.id, row.text, row.hash, row.source, row.label, row.sensitive, row.created_at],
            )? != 0;
            let item = if inserted { row.clone() } else {
                tx.query_row(&format!("SELECT {CLIPBOARD_COLUMNS} FROM clipboards WHERE hash=?1 ORDER BY created_at DESC, id DESC LIMIT 1"), [&row.hash], clipboard_from_row)?
            };
            tx.commit()?;
            Ok((item, inserted))
        })
    }

    pub fn clipboard_page(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> rusqlite::Result<Vec<ClipboardRow>> {
        let (clause, mut values) = search_filter(query);
        values.extend([
            Value::Integer(limit.clamp(1, 200)),
            Value::Integer(offset.max(0)),
        ]);
        self.with_conn(|c| {
            let mut statement = c.prepare(&format!("SELECT {CLIPBOARD_COLUMNS} FROM clipboards WHERE {clause} ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?"))?;
            statement.query_map(params_from_iter(values), clipboard_from_row)?.collect()
        })
    }

    pub fn clipboard_count(&self, query: &str) -> rusqlite::Result<i64> {
        let (clause, values) = search_filter(query);
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT COUNT(*) FROM clipboards WHERE {clause}"),
                params_from_iter(values),
                |row| row.get(0),
            )
        })
    }

    pub fn clipboard_delete_query(&self, query: &str) -> Result<usize, String> {
        let (clause, values) = bulk_filter(query)?;
        self.with_conn(|c| {
            c.execute(
                &format!("DELETE FROM clipboards WHERE {clause}"),
                params_from_iter(values),
            )
        })
        .map_err(|error| error.to_string())
    }
    pub fn clipboard_save(&self, row: &ClipboardRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO clipboards (id, text, hash, source, label, sensitive, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    row.id,
                    row.text,
                    row.hash,
                    row.source,
                    row.label,
                    row.sensitive as i64,
                    row.created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn clipboard_get(&self, id: &str) -> rusqlite::Result<Option<ClipboardRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {CLIPBOARD_COLUMNS} FROM clipboards WHERE id=?1"),
                [id],
                clipboard_from_row,
            )
            .optional()
        })
    }

    pub fn clipboard_latest(&self) -> rusqlite::Result<Option<ClipboardRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!(
                    "SELECT {CLIPBOARD_COLUMNS} FROM clipboards ORDER BY created_at DESC LIMIT 1"
                ),
                [],
                clipboard_from_row,
            )
            .optional()
        })
    }

    pub fn clipboard_latest_by_hash(&self, hash: &str) -> rusqlite::Result<Option<ClipboardRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!(
                    "SELECT {CLIPBOARD_COLUMNS} FROM clipboards WHERE hash=?1 ORDER BY created_at DESC LIMIT 1"
                ),
                [hash],
                clipboard_from_row,
            )
            .optional()
        })
    }

    pub fn clipboard_delete_by_ids(&self, ids: &[String]) -> rusqlite::Result<usize> {
        let (clause, values) = in_clause("id", ids);
        self.with_conn(|c| {
            c.execute(
                &format!("DELETE FROM clipboards WHERE {clause}"),
                params_from_iter(values),
            )
        })
    }

    pub fn clipboard_clear(&self) -> rusqlite::Result<usize> {
        self.with_conn(|c| c.execute("DELETE FROM clipboards", []))
    }

    pub fn clipboard_count_rows(&self) -> rusqlite::Result<i64> {
        self.with_conn(|c| c.query_row("SELECT COUNT(*) FROM clipboards", [], |r| r.get(0)))
    }
}

fn pattern(value: &str) -> Value {
    Value::Text(format!(
        "%{}%",
        value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    ))
}

fn search_filter(query: &str) -> (String, Vec<Value>) {
    if query.is_empty() {
        return ("1=1".into(), vec![]);
    }
    (
        "(text LIKE ? ESCAPE '\\' OR label LIKE ? ESCAPE '\\' OR source LIKE ? ESCAPE '\\')".into(),
        vec![pattern(query); 3],
    )
}

fn bulk_filter(query: &str) -> Result<(String, Vec<Value>), String> {
    if query.trim().is_empty() {
        return Err("query is required for bulk mutations — pass 'all:true' to explicitly target everything".into());
    }
    let mut clauses = vec![];
    let mut values = vec![];
    for field in crate::utils::search_dsl::parse(query) {
        match field.name.as_str() {
            "all" if field.value == "true" => {}
            "text" => {
                clauses.push("text LIKE ? ESCAPE '\\'".into());
                values.push(pattern(&field.value));
            }
            "ids" => {
                let ids = field
                    .value
                    .split(',')
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                let (clause, arguments) = in_clause("id", &ids);
                clauses.push(clause);
                values.extend(arguments);
            }
            _ => return Err(format!("unsupported clipboard filter: {}", field.name)),
        }
    }
    if clauses.is_empty() && crate::utils::search_dsl::parse(query).is_empty() {
        return Err("clipboard filter is required".into());
    }
    Ok((
        if clauses.is_empty() {
            "1=1".into()
        } else {
            clauses.join(" AND ")
        },
        values,
    ))
}

fn in_clause(column: &str, values: &[String]) -> (String, Vec<Value>) {
    if values.is_empty() {
        return ("0=1".into(), vec![]);
    }
    (
        format!("{column} IN ({})", vec!["?"; values.len()].join(",")),
        values.iter().cloned().map(Value::Text).collect(),
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/db/managers/clipboard.rs"]
mod tests;
