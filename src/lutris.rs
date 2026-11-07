use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::config;
use crate::game::{Game, Platform};

fn detect_platform(runner: &str, slug: &str) -> Platform {
    if slug.contains("warcraft-forever")
        || slug.contains("warcraft-classic-forever")
        || slug.contains("world-of-warcraft")
    {
        return Platform::Battlenet;
    }
    match runner {
        "steam" => Platform::Steam,
        "gog" => Platform::Gog,
        "epic" => Platform::Epic,
        "battlenet" => Platform::Battlenet,
        "lutris" | "wine" | "dosbox" | "native" | "scummvm" | "ppsspp" => Platform::Lutris,
        _ => {
            if slug.contains("battlenet") || runner.contains("battlenet") {
                Platform::Battlenet
            } else if slug.contains("gog") {
                Platform::Gog
            } else {
                Platform::Lutris
            }
        }
    }
}

pub fn normalise_slug(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(pos) = trimmed.rfind('-') {
        let tail = &trimmed[pos + 1..];
        if tail.len() >= 7 && tail.chars().all(|c| c.is_ascii_digit()) {
            return trimmed[..pos].to_string();
        }
    }
    trimmed.to_string()
}

fn find_lutris_art(slug: &str) -> Option<PathBuf> {
    let covers_dir = config::lutris_covers_dir();
    if !covers_dir.exists() {
        return None;
    }
    for ext in [".jpg", ".jpeg", ".png", ".webp"] {
        let p = covers_dir.join(format!("{}{}", slug, ext));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn extract_yaml_game_args(yaml_path: &Path) -> String {
    let Ok(txt) = fs::read_to_string(yaml_path) else {
        return String::new();
    };
    let mut in_game = false;
    for line in txt.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            in_game = trimmed == "game:" || trimmed.starts_with("game:");
            continue;
        }
        if in_game && (trimmed.starts_with("args:") || trimmed.starts_with("args :")) {
            let val = trimmed["args:".len()..].trim();
            return val.trim_matches(|c| c == '"' || c == '\'').to_string();
        }
    }
    String::new()
}

fn extract_digits_4plus(s: &str) -> Option<String> {
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            cur.push(c);
        } else {
            if cur.len() >= 4 {
                return Some(cur);
            }
            cur.clear();
        }
    }
    if cur.len() >= 4 {
        Some(cur)
    } else {
        None
    }
}

fn contains_word_ci(haystack: &str, word: &str) -> bool {
    let h = haystack.to_lowercase();
    let w = word.to_lowercase();
    for (idx, _) in h.match_indices(&w) {
        let before_ok = idx == 0
            || !h[..idx]
                .chars()
                .next_back()
                .map(|c| c.is_ascii_alphanumeric() || c == '_')
                .unwrap_or(false);
        let after_idx = idx + w.len();
        let after_ok = after_idx >= h.len()
            || !h[after_idx..]
                .chars()
                .next()
                .map(|c| c.is_ascii_alphanumeric() || c == '_')
                .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn import_from_pga(db_path: &Path) -> Vec<Game> {
    let mut games = Vec::new();
    let Ok(con) = Connection::open(db_path) else {
        eprintln!("[lutris] Warning: could not open pga.db");
        return games;
    };

    let Ok(mut stmt) = con.prepare(
        "SELECT id, name, slug, runner, directory, playtime, lastplayed, year, installed, configpath
         FROM games
         WHERE installed = 1
         ORDER BY id ASC",
    ) else {
        return games;
    };

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            row.get::<_, Option<f64>>(5)?.unwrap_or(0.0) as i64,
            row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            row.get::<_, Option<i64>>(7)?,
            row.get::<_, Option<String>>(9)?.unwrap_or_default(),
        ))
    });

    let Ok(rows) = rows else {
        return games;
    };

    for row in rows.flatten() {
        let (id, raw_name, raw_slug, raw_runner, directory, playtime, lastplayed, mut year, cfg_path) =
            row;
        let mut name = raw_name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let mut norm_slug = normalise_slug(&raw_slug);
        let runner = raw_runner.to_lowercase();

        let mut platform = detect_platform(&runner, &norm_slug);
        let art = find_lutris_art(&norm_slug).or_else(|| find_lutris_art(&raw_slug));
        let install_dir = if !directory.is_empty() {
            Some(PathBuf::from(&directory))
        } else {
            None
        };

        let mut args_exec = String::new();
        if !cfg_path.is_empty() {
            let yaml_p = config::lutris_games_dir2().join(format!("{}.yml", cfg_path));
            if yaml_p.exists() {
                args_exec = extract_yaml_game_args(&yaml_p);
            }
        }

        if args_exec.contains("launch OSI") {
            name = "Diablo II: Resurrected".to_string();
            norm_slug = "diablo-ii-resurrected".to_string();
            year = Some(2021);
            platform = Platform::Battlenet;
        } else if args_exec.contains("launch Fen") {
            name = "Diablo IV".to_string();
            norm_slug = "diablo-iv".to_string();
            year = Some(2023);
            platform = Platform::Battlenet;
        } else if contains_word_ci(&args_exec, "launch wowf")
            || contains_word_ci(&args_exec, "launch_uid wowf")
            || norm_slug.contains("warcraft-forever")
            || norm_slug.contains("warcraft-classic-forever")
        {
            if name.is_empty() || name == "Battle.net" {
                name = "World of Warcraft Forever".to_string();
            }
            norm_slug = if !raw_slug.is_empty() && raw_slug != "battlenet" {
                normalise_slug(&raw_slug)
            } else {
                "world-of-warcraft-forever".to_string()
            };
            year = Some(2026);
            platform = Platform::Battlenet;
        } else if args_exec.contains("launch WoW") {
            name = "World of Warcraft".to_string();
            norm_slug = "world-of-warcraft".to_string();
            year = Some(2004);
            platform = Platform::Battlenet;
        } else if args_exec.contains("launch WTCG") {
            name = "Hearthstone".to_string();
            norm_slug = "hearthstone".to_string();
            year = Some(2014);
            platform = Platform::Battlenet;
        } else if args_exec.contains("launch S2") {
            name = "StarCraft II".to_string();
            norm_slug = "starcraft-ii".to_string();
            year = Some(2010);
            platform = Platform::Battlenet;
        } else if args_exec.contains("launch D1") {
            name = "Diablo".to_string();
            norm_slug = "diablo".to_string();
            year = Some(1996);
            platform = Platform::Battlenet;
        } else if contains_word_ci(&args_exec, "launch W1R") {
            if name == "Battle.net" {
                name = "Warcraft: Remastered".to_string();
            }
            norm_slug = if !raw_slug.is_empty() && raw_slug != "battlenet" {
                normalise_slug(&raw_slug)
            } else {
                "warcraft-remastered".to_string()
            };
            year = Some(2024);
            platform = Platform::Battlenet;
        } else if contains_word_ci(&args_exec, "launch W2R") {
            if name == "Battle.net" {
                name = "Warcraft II: Remastered".to_string();
            }
            norm_slug = if !raw_slug.is_empty() && raw_slug != "battlenet" {
                normalise_slug(&raw_slug)
            } else {
                "warcraft-ii-remastered".to_string()
            };
            year = Some(2024);
            platform = Platform::Battlenet;
        } else if contains_word_ci(&args_exec, "launch W3") {
            if name == "Battle.net" {
                name = "Warcraft III: Reforged".to_string();
            }
            norm_slug = if !raw_slug.is_empty() && raw_slug != "battlenet" {
                normalise_slug(&raw_slug)
            } else {
                "warcraft-iii-reforged".to_string()
            };
            year = Some(2020);
            platform = Platform::Battlenet;
        }

        if year == Some(1996) {
            if norm_slug.contains("hearthstone") {
                year = Some(2014);
            } else if norm_slug.contains("diablo-iv") {
                year = Some(2023);
            } else if norm_slug.contains("starcraft-ii") {
                year = Some(2010);
            } else if norm_slug.contains("world-of-warcraft-forever") {
                year = Some(2026);
            } else if norm_slug.contains("world-of-warcraft") {
                year = Some(2004);
            } else if norm_slug.contains("battlenet") {
                year = None;
            }
        }

        let steam_appid = if runner == "steam" {
            extract_digits_4plus(&norm_slug).or_else(|| extract_digits_4plus(&cfg_path))
        } else {
            None
        };

        let slug = if norm_slug.is_empty() {
            format!("lutris-{}", id)
        } else {
            norm_slug
        };

        games.push(Game {
            slug,
            name,
            platform,
            steam_appid,
            is_shortcut: false,
            lutris_id: Some(id),
            lutris_slug: Some(raw_slug),
            runner,
            art_path: art,
            install_dir,
            playtime_minutes: playtime,
            last_played: lastplayed,
            year,
            launch_via_lutris: true,
            lutris_launch_id: Some(id),
            mtime: id as f64,
        });
    }

    println!("[lutris] pga.db: found {} installed games.", games.len());
    games
}

fn deduplicate(games: Vec<Game>) -> Vec<Game> {
    let mut best: HashMap<String, (usize, Game)> = HashMap::new();
    let mut order = 0usize;
    for g in games {
        match best.get_mut(&g.slug) {
            Some((_, existing)) => {
                if g.mtime > existing.mtime {
                    *existing = g;
                }
            }
            None => {
                best.insert(g.slug.clone(), (order, g));
                order += 1;
            }
        }
    }
    let mut list: Vec<(usize, Game)> = best.into_values().collect();
    list.sort_by_key(|(idx, _)| *idx);
    list.into_iter().map(|(_, g)| g).collect()
}

pub fn import_games() -> Vec<Game> {
    let db = config::lutris_pga_db();
    let games = if db.exists() {
        import_from_pga(&db)
    } else {
        Vec::new()
    };
    let deduped = deduplicate(games);
    println!("[lutris] After dedup: {} unique games.", deduped.len());
    deduped
}
