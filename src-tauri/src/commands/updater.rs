use serde::Serialize;

const GITHUB_REPO: &str = "plainhub/plain-desktop";
const RELEASES_URL: &str = "https://api.github.com/repos/plainhub/plain-desktop/releases/latest";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    current_version: String,
    latest_version: String,
    has_update: bool,
    release_name: String,
    release_url: String,
    published_at: String,
}

#[tauri::command]
pub async fn check_for_updates(app: tauri::AppHandle) -> Result<UpdateCheck, String> {
    let current = app.package_info().version.to_string();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(RELEASES_URL)
        .header(reqwest::header::USER_AGENT, "PlainApp")
        .send()
        .await
        .map_err(|e| format!("Failed to reach GitHub: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub API returned HTTP {}", resp.status()));
    }
    let body = resp.text().await.map_err(|e| format!("Failed to read response: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid response from GitHub: {e}"))?;

    let latest_tag = json.get("tag_name").and_then(|v| v.as_str()).unwrap_or("");
    let latest_version = extract_version(latest_tag).unwrap_or_default();
    if latest_version.is_empty() {
        return Err(format!("No valid version found in release tag: {latest_tag}"));
    }
    let has_update = compare_versions(&latest_version, &current) == std::cmp::Ordering::Greater;
    Ok(UpdateCheck {
        current_version: current.to_string(),
        latest_version,
        has_update,
        release_name: json.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        release_url: json
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or(GITHUB_REPO)
            .to_string(),
        published_at: json.get("published_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    })
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .map(|p| p.split('-').next().unwrap_or(p))
            .filter_map(|p| p.trim().parse().ok())
            .collect()
    };
    let a = parts(a);
    let b = parts(b);
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
    }
    std::cmp::Ordering::Equal
}

/// Extract the `X.Y.Z` version number embedded in a release tag, e.g. from
/// `vdesktop-v0.1.0` returns `0.1.0`. Returns `None` when no numeric version
/// is found.
fn extract_version(tag: &str) -> Option<String> {
    let chars: Vec<char> = tag.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            let candidate: String = chars[i..j].iter().collect();
            if candidate.matches('.').count() >= 1 && !candidate.ends_with('.') {
                return Some(candidate);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{compare_versions, extract_version};
    use std::cmp::Ordering;

    #[test]
    fn compares_numeric_versions() {
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.2.0", "0.1.0"), Ordering::Greater);
        assert_eq!(compare_versions("0.1.0", "0.2.0"), Ordering::Less);
        assert_eq!(compare_versions("1.0", "0.9.9"), Ordering::Greater);
    }

    #[test]
    fn ignores_v_prefix_and_prerelease_suffix() {
        assert_eq!(compare_versions("v1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(compare_versions("1.2.3-beta", "1.2.2"), Ordering::Greater);
    }

    #[test]
    fn extracts_version_from_arbitrary_tag() {
        assert_eq!(extract_version("vdesktop-v0.1.0").as_deref(), Some("0.1.0"));
        assert_eq!(extract_version("desktop-v1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(extract_version("v0.1.0").as_deref(), Some("0.1.0"));
        assert_eq!(extract_version("0.1.0-beta").as_deref(), Some("0.1.0"));
        assert_eq!(extract_version("no-version-here"), None);
    }

    #[test]
    fn same_version_is_not_an_update() {
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Ordering::Equal);
    }
}