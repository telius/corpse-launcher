use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use crate::config;

const BASE: &str = "https://www.steamgriddb.com/api/v2";

pub fn has_key() -> bool {
    !config::sgdb_api_key().is_empty()
}

pub fn cache_path(game_slug: &str) -> PathBuf {
    let digest = md5::compute(game_slug.as_bytes());
    let hex_full = format!("{:x}", digest);
    let safe = &hex_full[..12.min(hex_full.len())];
    let slug_part: String = game_slug.chars().take(32).collect();
    config::art_cache_dir().join(format!("{}_{}.jpg", safe, slug_part))
}

fn get_json(url: &str) -> Option<serde_json::Value> {
    let key = config::sgdb_api_key();
    if key.is_empty() {
        return None;
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    let resp = agent
        .get(url)
        .set("Authorization", &format!("Bearer {}", key))
        .call()
        .ok()?;
    resp.into_json().ok()
}

pub fn download_url_to_path(url: &str, dest: &PathBuf) -> bool {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let Ok(resp) = agent.get(url).call() else {
        return false;
    };
    if resp.status() != 200 {
        return false;
    }
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut bytes = Vec::new();
    if resp.into_reader().read_to_end(&mut bytes).is_err() || bytes.is_empty() {
        return false;
    }
    fs::write(dest, &bytes).is_ok()
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn get_game_id_by_steam_appid(appid: &str) -> Option<i64> {
    let url = format!("{}/games/steam/{}", BASE, appid);
    let data = get_json(&url)?;
    if data.get("success")?.as_bool()? {
        data.get("data")?.get("id")?.as_i64()
    } else {
        None
    }
}

fn get_game_id_by_name(name: &str) -> Option<i64> {
    let url = format!("{}/search/autocomplete/{}", BASE, urlencode(name));
    let data = get_json(&url)?;
    if data.get("success")?.as_bool()? {
        data.get("data")?.as_array()?.first()?.get("id")?.as_i64()
    } else {
        None
    }
}

pub fn get_sgdb_id(name: &str, steam_appid: Option<&str>) -> Option<i64> {
    let mut sgdb_id = None;
    if let Some(appid) = steam_appid {
        if let Ok(val) = appid.parse::<u64>() {
            if val <= 0xFFFF_FFFF {
                sgdb_id = get_game_id_by_steam_appid(appid);
            }
        }
    }
    if sgdb_id.is_none() && !name.is_empty() {
        sgdb_id = get_game_id_by_name(name);
    }
    sgdb_id
}

pub fn fetch_all_grid_urls(sgdb_id: i64, style: &str, dimensions: &str) -> Vec<String> {
    let mut url = format!(
        "{}/grids/game/{}?dimensions={}&mimes={}",
        BASE,
        sgdb_id,
        urlencode(dimensions),
        urlencode("image/png,image/jpeg,image/webp")
    );
    if !style.is_empty() {
        url.push_str(&format!("&styles={}", urlencode(style)));
    }
    let mut urls = Vec::new();
    if let Some(data) = get_json(&url) {
        if data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            if let Some(arr) = data.get("data").and_then(|v| v.as_array()) {
                for item in arr {
                    let nsfw = item.get("nsfw").and_then(|v| v.as_bool()).unwrap_or(false);
                    let humor = item.get("humor").and_then(|v| v.as_bool()).unwrap_or(false);
                    if !nsfw && !humor {
                        if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
                            urls.push(u.to_string());
                        }
                    }
                }
            }
        }
    }
    urls
}

fn fetch_grid_url(sgdb_id: i64, style: &str, dimensions: &str) -> Option<String> {
    for st in [Some(style), None] {
        let mut url = format!(
            "{}/grids/game/{}?dimensions={}&mimes={}",
            BASE,
            sgdb_id,
            urlencode(dimensions),
            urlencode("image/png,image/jpeg,image/webp")
        );
        if let Some(s) = st {
            if !s.is_empty() {
                url.push_str(&format!("&styles={}", urlencode(s)));
            }
        }
        if let Some(data) = get_json(&url) {
            if data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                if let Some(arr) = data.get("data").and_then(|v| v.as_array()) {
                    for item in arr {
                        let nsfw = item.get("nsfw").and_then(|v| v.as_bool()).unwrap_or(false);
                        let humor = item.get("humor").and_then(|v| v.as_bool()).unwrap_or(false);
                        if !nsfw && !humor {
                            if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
                                return Some(u.to_string());
                            }
                        }
                    }
                    if let Some(first) = arr.first().and_then(|i| i.get("url")).and_then(|v| v.as_str()) {
                        return Some(first.to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn fetch_art(
    game_slug: &str,
    steam_appid: Option<&str>,
    game_name: &str,
) -> Option<PathBuf> {
    if !has_key() {
        return None;
    }
    let dest = cache_path(game_slug);
    if let Ok(meta) = fs::metadata(&dest) {
        if meta.len() > 0 {
            return Some(dest);
        }
    }
    let sgdb_id = get_sgdb_id(game_name, steam_appid)?;
    let style = config::get_str("art", "sgdb_style");
    let dimensions = config::get_str("art", "sgdb_dimensions");
    let url = fetch_grid_url(sgdb_id, &style, &dimensions)?;
    if download_url_to_path(&url, &dest) {
        Some(dest)
    } else {
        None
    }
}
