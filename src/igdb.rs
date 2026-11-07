use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config;

const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const API_URL: &str = "https://api.igdb.com/v4";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IGDBMetadata {
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub companies: Vec<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub release_year: Option<i64>,
}

fn cache_path() -> PathBuf {
    config::cache_dir().join("igdb_cache.json")
}

static META_CACHE: LazyLock<Mutex<HashMap<String, IGDBMetadata>>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    let p = cache_path();
    if p.exists() {
        if let Ok(txt) = fs::read_to_string(&p) {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, IGDBMetadata>>(&txt) {
                map = parsed;
            }
        }
    }
    Mutex::new(map)
});

static TOKEN_STATE: LazyLock<Mutex<(Option<String>, f64)>> =
    LazyLock::new(|| Mutex::new((None, 0.0)));

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs_f64()
}

fn save_cache(map: &HashMap<String, IGDBMetadata>) {
    let p = cache_path();
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(txt) = serde_json::to_string_pretty(map) {
        let _ = fs::write(&p, txt);
    }
}

pub fn get_cached_metadata(slug: &str) -> Option<IGDBMetadata> {
    META_CACHE.lock().ok()?.get(slug).cloned()
}

fn get_access_token() -> Option<String> {
    let now = now_secs();
    if let Ok(guard) = TOKEN_STATE.lock() {
        if let Some(ref tok) = guard.0 {
            if now < guard.1 {
                return Some(tok.clone());
            }
        }
    }

    let (client_id, client_secret) = config::igdb_credentials();
    if client_id.is_empty() || client_secret.is_empty() {
        return None;
    }

    let url = format!(
        "{}?client_id={}&client_secret={}&grant_type=client_credentials",
        TOKEN_URL, client_id, client_secret
    );
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    let resp = agent.post(&url).call().ok()?;
    let json: serde_json::Value = resp.into_json().ok()?;
    let tok = json.get("access_token")?.as_str()?.to_string();
    let expires_in = json
        .get("expires_in")
        .and_then(|v| v.as_f64())
        .unwrap_or(3600.0);
    if let Ok(mut guard) = TOKEN_STATE.lock() {
        *guard = (Some(tok.clone()), now + expires_in - 60.0);
    }
    Some(tok)
}

fn timestamp_to_year(ts: i64) -> Option<i64> {
    unsafe {
        let t = ts as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if !libc::gmtime_r(&t, &mut tm).is_null() {
            return Some((tm.tm_year + 1900) as i64);
        }
    }
    None
}

pub fn fetch_metadata(game_name: &str, slug: &str) -> Option<IGDBMetadata> {
    if let Some(cached) = get_cached_metadata(slug) {
        return Some(cached);
    }

    let (client_id, _) = config::igdb_credentials();
    let token = get_access_token()?;
    if client_id.is_empty() {
        return None;
    }

    let clean_name = game_name
        .replace(['®', '™', '©'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let query = format!(
        "search \"{}\"; fields name, genres.name, involved_companies.company.name, involved_companies.developer, involved_companies.publisher, summary, total_rating, first_release_date; limit 5;",
        clean_name
    );

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    let resp = agent
        .post(&format!("{}/games", API_URL))
        .set("Client-ID", &client_id)
        .set("Authorization", &format!("Bearer {}", token))
        .set("Content-Type", "text/plain")
        .send_string(&query)
        .ok()?;

    let games_list: Vec<serde_json::Value> = resp.into_json().ok()?;
    if games_list.is_empty() {
        return None;
    }

    let mut chosen = &games_list[0];
    for g in &games_list {
        if let Some(n) = g.get("name").and_then(|v| v.as_str()) {
            if n.eq_ignore_ascii_case(game_name) {
                chosen = g;
                break;
            }
        }
    }
    if chosen
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| !s.eq_ignore_ascii_case(game_name))
        .unwrap_or(true)
    {
        for g in &games_list {
            if let Some(n) = g.get("name").and_then(|v| v.as_str()) {
                if !n.contains(':') {
                    chosen = g;
                    break;
                }
            }
        }
    }

    let mut genres = Vec::new();
    if let Some(arr) = chosen.get("genres").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                genres.push(name.to_string());
            }
        }
    }
    genres.truncate(3);

    let mut devs = Vec::new();
    let mut pubs = Vec::new();
    if let Some(arr) = chosen.get("involved_companies").and_then(|v| v.as_array()) {
        for ic in arr {
            if let Some(cname) = ic
                .get("company")
                .and_then(|c| c.get("name"))
                .and_then(|v| v.as_str())
            {
                if ic.get("developer").and_then(|v| v.as_bool()).unwrap_or(false) {
                    devs.push(cname.to_string());
                } else if ic.get("publisher").and_then(|v| v.as_bool()).unwrap_or(false) {
                    pubs.push(cname.to_string());
                }
            }
        }
    }
    let mut companies = if !devs.is_empty() { devs } else { pubs };
    companies.truncate(2);

    let release_year = chosen
        .get("first_release_date")
        .and_then(|v| v.as_i64())
        .and_then(timestamp_to_year);

    let summary = chosen
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let rating = chosen.get("total_rating").and_then(|v| v.as_f64());

    let meta = IGDBMetadata {
        genres,
        companies,
        summary,
        rating,
        release_year,
    };

    if let Ok(mut guard) = META_CACHE.lock() {
        guard.insert(slug.to_string(), meta.clone());
        save_cache(&guard);
    }

    Some(meta)
}
