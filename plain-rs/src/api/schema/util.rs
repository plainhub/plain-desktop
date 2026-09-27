use std::io::{BufRead, BufReader};

/// Read `limit` lines from `path` starting at `offset`, keeping only lines
/// containing `text` (empty `text` keeps everything).
pub fn read_log_lines(path: &std::path::Path, text: &str, offset: i32, limit: i32) -> Vec<String> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let text = text.trim();
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| text.is_empty() || line.contains(text))
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect()
}
