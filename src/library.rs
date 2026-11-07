use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::config;
use crate::game::{Game, Platform};
use crate::lutris;
use crate::steam;

static LIBRARY_CACHE: LazyLock<Mutex<Option<Vec<Game>>>> = LazyLock::new(|| Mutex::new(None));

fn merge(steam_games: Vec<Game>, lutris_games: Vec<Game>) -> Vec<Game> {
    let mut result: HashMap<String, Game> = HashMap::new();
    let mut steam_by_appid: HashMap<String, Game> = HashMap::new();
    for g in &steam_games {
        if let Some(ref appid) = g.steam_appid {
            steam_by_appid.insert(appid.clone(), g.clone());
        }
    }

    let mut lutris_by_slug: HashMap<String, Game> = HashMap::new();
    let mut lutris_by_appid: HashMap<String, Game> = HashMap::new();
    for g in &lutris_games {
        lutris_by_slug.insert(g.slug.clone(), g.clone());
        if g.runner == "steam" {
            if let Some(ref appid) = g.steam_appid {
                lutris_by_appid.insert(appid.clone(), g.clone());
            }
        }
    }

    for sg in steam_games {
        let lg_appid = sg
            .steam_appid
            .as_ref()
            .and_then(|id| lutris_by_appid.get(id));
        let lg_slug = lutris_by_slug.get(&sg.slug);
        let lg = lg_appid.or(lg_slug);

        if let Some(lg) = lg {
            let platform = if lg.platform != Platform::Steam || sg.is_shortcut {
                lg.platform
            } else {
                sg.platform
            };
            let merged = Game {
                slug: sg.slug.clone(),
                name: if !sg.name.is_empty() {
                    sg.name.clone()
                } else {
                    lg.name.clone()
                },
                platform,
                steam_appid: sg.steam_appid.clone(),
                is_shortcut: sg.is_shortcut,
                lutris_id: lg.lutris_id,
                lutris_slug: lg.lutris_slug.clone(),
                runner: if !lg.runner.is_empty() {
                    lg.runner.clone()
                } else {
                    sg.runner.clone()
                },
                art_path: sg.art_path.clone().or_else(|| lg.art_path.clone()),
                install_dir: sg.install_dir.clone().or_else(|| lg.install_dir.clone()),
                playtime_minutes: sg.playtime_minutes.max(lg.playtime_minutes),
                last_played: sg.last_played.max(lg.last_played),
                year: sg.year.or(lg.year),
                launch_via_lutris: true,
                lutris_launch_id: lg.lutris_id,
                mtime: sg.mtime.max(lg.mtime),
            };
            result.insert(merged.slug.clone(), merged);
        } else {
            result.insert(sg.slug.clone(), sg);
        }
    }

    for lg in lutris_games {
        if result.contains_key(&lg.slug) {
            continue;
        }
        if let Some(ref appid) = lg.steam_appid {
            if steam_by_appid.contains_key(appid) {
                continue;
            }
        }
        result.insert(lg.slug.clone(), lg);
    }

    result.into_values().collect()
}

fn sort_games(mut games: Vec<Game>) -> Vec<Game> {
    let sort_by = config::get_str("general", "sort_by");
    match sort_by.as_str() {
        "playtime" => {
            games.sort_by(|a, b| b.playtime_minutes.cmp(&a.playtime_minutes));
        }
        "last_played" => {
            games.sort_by(|a, b| b.last_played.cmp(&a.last_played));
        }
        _ => {
            games.sort_by_cached_key(|g| g.name.to_lowercase());
        }
    }
    games
}

pub fn load(force: bool) -> Vec<Game> {
    if !force {
        if let Ok(guard) = LIBRARY_CACHE.lock() {
            if let Some(ref cached) = *guard {
                return cached.clone();
            }
        }
    }

    println!("[library] Importing Steam games...");
    let steam_games = steam::import_games();

    println!("[library] Importing Lutris games...");
    let lutris_games = lutris::import_games();

    println!("[library] Merging libraries...");
    let merged = merge(steam_games, lutris_games);
    let sorted = sort_games(merged);

    println!("[library] Total unique games: {}", sorted.len());
    if let Ok(mut guard) = LIBRARY_CACHE.lock() {
        *guard = Some(sorted.clone());
    }
    sorted
}

pub fn invalidate() {
    if let Ok(mut guard) = LIBRARY_CACHE.lock() {
        *guard = None;
    }
}
