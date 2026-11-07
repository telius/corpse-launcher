use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::config;
use crate::game::{Game, Platform};
use crate::hidden;
use crate::igdb;
use crate::vdf::{self, VdfValue};

fn find_steam_root() -> Option<PathBuf> {
    let home = config::home_dir();
    let candidates = [
        config::steam_root(),
        home.join(".local").join("share").join("Steam"),
        home.join(".steam").join("steam"),
        home.join(".var")
            .join("app")
            .join("com.valvesoftware.Steam")
            .join(".local")
            .join("share")
            .join("Steam"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn load_library_folders(steam_root: &Path) -> Vec<PathBuf> {
    let lf_path = steam_root.join("config").join("libraryfolders.vdf");
    if !lf_path.exists() {
        return vec![steam_root.join("steamapps")];
    }
    let Ok(txt) = fs::read_to_string(&lf_path) else {
        return vec![steam_root.join("steamapps")];
    };
    let parsed = vdf::parse_text(&txt);
    let mut dirs = Vec::new();
    if let Some(root) = parsed.get_ci("libraryfolders").and_then(|v| v.as_map()) {
        let mut keys: Vec<&String> = root
            .keys()
            .filter(|k| k.chars().all(|c| c.is_ascii_digit()))
            .collect();
        keys.sort_by_key(|k| k.parse::<u32>().unwrap_or(0));
        for k in keys {
            if let Some(val) = root.get(k) {
                let path_str = match val {
                    VdfValue::Str(s) => s.as_str(),
                    VdfValue::Map(_) => val.get_ci("path").and_then(|v| v.as_str()).unwrap_or(""),
                    _ => "",
                };
                if !path_str.is_empty() {
                    let p = PathBuf::from(path_str).join("steamapps");
                    if p.exists() {
                        dirs.push(p);
                    }
                }
            }
        }
    }
    if dirs.is_empty() {
        dirs.push(steam_root.join("steamapps"));
    }
    dirs
}

fn load_playtimes(steam_root: &Path) -> HashMap<String, (i64, i64)> {
    let mut playtimes = HashMap::new();
    let userdata = steam_root.join("userdata");
    let Ok(entries) = fs::read_dir(&userdata) else {
        return playtimes;
    };
    for entry in entries.flatten() {
        let lc = entry.path().join("config").join("localconfig.vdf");
        if !lc.exists() {
            continue;
        }
        let Ok(txt) = fs::read_to_string(&lc) else {
            continue;
        };
        let parsed = vdf::parse_text(&txt);
        if let Some(apps) = parsed
            .get_ci("UserLocalConfigStore")
            .and_then(|v| v.get_ci("Software"))
            .and_then(|v| v.get_ci("Valve"))
            .and_then(|v| v.get_ci("Steam"))
            .and_then(|v| v.get_ci("apps"))
            .and_then(|v| v.as_map())
        {
            for (appid, info) in apps {
                let pt = info
                    .get_ci("Playtime")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let lp = info
                    .get_ci("LastPlayed")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if pt != 0 || lp != 0 {
                    playtimes.insert(appid.clone(), (pt, lp));
                }
            }
        }
    }
    playtimes
}

fn find_steam_art(appid: &str, steam_root: &Path, icon: &str) -> Option<PathBuf> {
    let mut cids = vec![appid.to_string()];
    if let Ok(val) = appid.parse::<u64>() {
        if val > 0xFFFF_FFFF {
            cids.push((val >> 32).to_string());
        } else if val > 0 {
            cids.push(((val << 32) | 0x0200_0000).to_string());
        }
    }

    let userdata = steam_root.join("userdata");
    let grid_dirs: Vec<PathBuf> = fs::read_dir(&userdata)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("config").join("grid"))
        .filter(|g| g.exists())
        .collect();

    for grid in &grid_dirs {
        for cid in &cids {
            for suffix in ["p.png", "p.jpg", ".png", ".jpg"] {
                let p = grid.join(format!("{}{}", cid, suffix));
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    let art_dir = config::steam_art_cache();
    if art_dir.exists() {
        for cid in &cids {
            for suffix in [
                format!("{}_library_600x900.jpg", cid),
                format!("{}_library_600x900.png", cid),
                format!("{}_library_hero.jpg", cid),
                format!("{}_header.jpg", cid),
            ] {
                let p = art_dir.join(suffix);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    for grid in &grid_dirs {
        for cid in &cids {
            for suffix in ["_hero.png", "_hero.jpg", "_header.png", "_header.jpg"] {
                let p = grid.join(format!("{}{}", cid, suffix));
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    if !icon.is_empty() {
        let trimmed = icon.trim_matches(|c| c == ' ' || c == '"' || c == '\'');
        let ip = PathBuf::from(trimmed);
        if ip.exists() && ip.is_file() {
            return Some(ip);
        }
    }

    None
}

pub fn make_slug(name: &str, appid: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        format!("steam-{}", appid)
    } else {
        slug
    }
}

fn file_mtime(path: &Path) -> f64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn load_shortcuts(steam_root: &Path, playtimes: &HashMap<String, (i64, i64)>) -> Vec<Game> {
    let userdata = steam_root.join("userdata");
    let Ok(entries) = fs::read_dir(&userdata) else {
        return Vec::new();
    };
    let mut uid_dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s != "0" && s.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false)
        })
        .collect();
    uid_dirs.sort();

    let mut games = Vec::new();
    let mut seen_slugs = HashSet::new();

    for uid_dir in uid_dirs {
        let sc_path = uid_dir.join("config").join("shortcuts.vdf");
        if !sc_path.exists() {
            continue;
        }
        let Ok(bytes) = fs::read(&sc_path) else {
            continue;
        };
        let parsed = vdf::parse_binary(&bytes);
        let Some(sc_map) = parsed.get_ci("shortcuts").and_then(|v| v.as_map()) else {
            continue;
        };

        let mut keys: Vec<&String> = sc_map.keys().collect();
        keys.sort_by_key(|k| k.parse::<u32>().unwrap_or(u32::MAX));

        let mtime = file_mtime(&sc_path);

        for k in keys {
            let Some(item) = sc_map.get(k) else {
                continue;
            };
            let name = item
                .get_ci("AppName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let exe = item
                .get_ci("Exe")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            let raw_appid = item.get_ci("appid").and_then(|v| v.as_i64());
            let mut u_appid: u64 = raw_appid.map(|a| (a as u64) & 0xFFFF_FFFF).unwrap_or(0);
            if u_appid == 0 {
                let mut combined = String::with_capacity(exe.len() + name.len());
                combined.push_str(&exe);
                combined.push_str(&name);
                let crc = crc32fast::hash(combined.as_bytes()) as u64;
                u_appid = crc | 0x8000_0000;
            }

            let rungameid = (u_appid << 32) | 0x0200_0000;
            let appid_str = rungameid.to_string();
            let u_appid_str = u_appid.to_string();

            let slug = make_slug(&name, &u_appid_str);
            if seen_slugs.contains(&slug) {
                continue;
            }

            let is_hid = item
                .get_ci("IsHidden")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if is_hid != 0 && !hidden::is_hidden(&slug) {
                hidden::hide(&slug);
            }

            let start_dir = item
                .get_ci("StartDir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_matches(|c| c == ' ' || c == '"' || c == '\'');
            let clean_exe = exe.trim_matches(|c| c == ' ' || c == '"' || c == '\'');

            let install_dir = if !start_dir.is_empty() && Path::new(start_dir).exists() {
                Some(PathBuf::from(start_dir))
            } else if !clean_exe.is_empty() {
                Path::new(clean_exe)
                    .parent()
                    .filter(|p| p.exists())
                    .map(|p| p.to_path_buf())
            } else {
                None
            };

            let icon = item.get_ci("icon").and_then(|v| v.as_str()).unwrap_or("");
            let art = find_steam_art(&appid_str, steam_root, icon);

            let (playtime, lc_last_played) = playtimes
                .get(&appid_str)
                .copied()
                .or_else(|| playtimes.get(&u_appid_str).copied())
                .unwrap_or((0, 0));
            let last_played = item
                .get_ci("LastPlayTime")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .max(lc_last_played);

            let year = igdb::get_cached_metadata(&slug).and_then(|m| m.release_year);

            games.push(Game {
                slug: slug.clone(),
                name,
                platform: Platform::Steam,
                steam_appid: Some(appid_str),
                is_shortcut: true,
                lutris_id: None,
                lutris_slug: None,
                runner: "steam".to_string(),
                art_path: art,
                install_dir,
                playtime_minutes: playtime,
                last_played,
                year,
                launch_via_lutris: false,
                lutris_launch_id: None,
                mtime,
            });
            seen_slugs.insert(slug);
        }
    }

    games
}

pub fn import_games() -> Vec<Game> {
    let Some(steam_root) = find_steam_root() else {
        println!("[steam] Steam installation not found.");
        return Vec::new();
    };

    let libraries = load_library_folders(&steam_root);
    let playtimes = load_playtimes(&steam_root);
    let mut games = Vec::new();

    const SKIP_PREFIXES: &[&str] = &[
        "Proton",
        "Steam Linux Runtime",
        "Steamworks",
        "Steam VR",
        "SteamVR",
    ];

    for steamapps in libraries {
        let Ok(entries) = fs::read_dir(&steamapps) else {
            continue;
        };
        let mut acf_paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.starts_with("appmanifest_") && s.ends_with(".acf"))
                    .unwrap_or(false)
            })
            .collect();
        acf_paths.sort();

        for acf_path in acf_paths {
            let Ok(txt) = fs::read_to_string(&acf_path) else {
                continue;
            };
            let parsed = vdf::parse_text(&txt);
            let Some(data) = parsed.get_ci("AppState") else {
                continue;
            };

            let appid = data
                .get_ci("appid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = data
                .get_ci("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let state = data
                .get_ci("StateFlags")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            if name.is_empty() || appid == "228980" || appid == "1070560" {
                continue;
            }
            if !matches!(state, 4 | 6 | 1030) {
                continue;
            }
            if SKIP_PREFIXES.iter().any(|p| name.starts_with(p)) {
                continue;
            }

            let install_dir_str = data
                .get_ci("installdir")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let install_dir = if !install_dir_str.is_empty() {
                Some(steamapps.join("common").join(install_dir_str))
            } else {
                None
            };

            let slug = make_slug(&name, &appid);
            let art = find_steam_art(&appid, &steam_root, "");
            let year = igdb::get_cached_metadata(&slug).and_then(|m| m.release_year);

            let (playtime_minutes, last_played) =
                playtimes.get(&appid).copied().unwrap_or((0, 0));

            games.push(Game {
                slug,
                name,
                platform: Platform::Steam,
                steam_appid: Some(appid.clone()),
                is_shortcut: false,
                lutris_id: None,
                lutris_slug: None,
                runner: "steam".to_string(),
                art_path: art,
                install_dir,
                playtime_minutes,
                last_played,
                year,
                launch_via_lutris: false,
                lutris_launch_id: None,
                mtime: file_mtime(&acf_path),
            });
        }
    }

    let official_count = games.len();
    let shortcuts = load_shortcuts(&steam_root, &playtimes);
    let mut existing_slugs: HashSet<String> = games.iter().map(|g| g.slug.clone()).collect();
    let mut existing_appids: HashSet<String> = games
        .iter()
        .filter_map(|g| g.steam_appid.clone())
        .collect();
    let mut shortcut_count = 0;

    for sc in shortcuts {
        let appid_dup = sc
            .steam_appid
            .as_ref()
            .map(|id| existing_appids.contains(id))
            .unwrap_or(false);
        if !existing_slugs.contains(&sc.slug) && !appid_dup {
            existing_slugs.insert(sc.slug.clone());
            if let Some(ref id) = sc.steam_appid {
                existing_appids.insert(id.clone());
            }
            games.push(sc);
            shortcut_count += 1;
        }
    }

    println!(
        "[steam] Imported {} games ({} official, {} shortcuts).",
        games.len(),
        official_count,
        shortcut_count
    );
    games
}
