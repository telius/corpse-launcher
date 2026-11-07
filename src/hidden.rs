use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use crate::config;

fn hidden_file() -> PathBuf {
    config::config_dir().join("hidden.txt")
}

static HIDDEN: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    let path = hidden_file();
    if path.exists() {
        if let Ok(txt) = fs::read_to_string(&path) {
            for line in txt.lines() {
                let s = line.trim();
                if !s.is_empty() {
                    set.insert(s.to_string());
                }
            }
        }
    }
    Mutex::new(set)
});

fn save_locked(set: &HashSet<String>) {
    let path = hidden_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut sorted: Vec<&str> = set.iter().map(|s| s.as_str()).collect();
    sorted.sort_unstable();
    let body = format!("{}\n", sorted.join("\n"));
    if let Err(e) = fs::write(&path, body) {
        eprintln!("[hidden] Could not save hidden list: {}", e);
    }
}

pub fn is_hidden(slug: &str) -> bool {
    HIDDEN
        .lock()
        .map(|s| s.contains(slug))
        .unwrap_or(false)
}

pub fn hide(slug: &str) {
    if let Ok(mut set) = HIDDEN.lock() {
        set.insert(slug.to_string());
        save_locked(&set);
        println!("[hidden] Hidden: {}", slug);
    }
}

pub fn unhide(slug: &str) {
    if let Ok(mut set) = HIDDEN.lock() {
        set.remove(slug);
        save_locked(&set);
        println!("[hidden] Unhidden: {}", slug);
    }
}

pub fn toggle(slug: &str) -> bool {
    if is_hidden(slug) {
        unhide(slug);
        false
    } else {
        hide(slug);
        true
    }
}
