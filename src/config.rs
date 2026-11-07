use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

pub type ColorTuple = (u8, u8, u8);

pub const BG: ColorTuple = (0, 0, 0);
pub const BG_DEEP: ColorTuple = (0, 0, 0);
pub const BG_PANEL: ColorTuple = (22, 22, 26);
pub const BG_CARD: ColorTuple = (22, 22, 26);

pub const VIOLET: ColorTuple = (102, 0, 255);
pub const MAGENTA: ColorTuple = (255, 0, 255);
pub const CYAN: ColorTuple = (0, 255, 255);
pub const WHITE: ColorTuple = (240, 240, 255);
pub const GREY: ColorTuple = (100, 80, 140);
pub const BORDER_BG: ColorTuple = (0, 0, 0);

pub const BADGE_STEAM: ColorTuple = (102, 192, 234);
pub const BADGE_LUTRIS: ColorTuple = (255, 167, 38);
pub const BADGE_BN: ColorTuple = (0, 164, 255);

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/james"))
}

pub fn config_dir() -> PathBuf {
    home_dir().join(".config").join("corpse-launcher")
}

pub fn art_cache_dir() -> PathBuf {
    config_dir().join("art")
}

pub fn custom_art_dir() -> PathBuf {
    config_dir().join("custom_art")
}

pub fn cache_dir() -> PathBuf {
    home_dir().join(".cache").join("corpse-launcher")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn steam_root() -> PathBuf {
    home_dir().join(".steam").join("root")
}

pub fn steam_art_cache() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("Steam")
        .join("appcache")
        .join("librarycache")
}

pub fn lutris_pga_db() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("lutris")
        .join("pga.db")
}

pub fn lutris_covers_dir() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("lutris")
        .join("covers")
}

pub fn lutris_games_dir2() -> PathBuf {
    home_dir().join(".config").join("lutris").join("games")
}

#[derive(Clone, Debug)]
pub enum ConfigVal {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl ConfigVal {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigVal::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ConfigVal::Int(i) => Some(*i),
            ConfigVal::Float(f) => Some(*f as i64),
            _ => None,
        }
    }
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ConfigVal::Float(f) => Some(*f),
            ConfigVal::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ConfigVal::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

pub type SectionMap = Vec<(String, Vec<(String, ConfigVal)>)>;

fn default_sections() -> SectionMap {
    vec![
        (
            "general".to_string(),
            vec![
                ("window_width".to_string(), ConfigVal::Int(1280)),
                ("window_height".to_string(), ConfigVal::Int(720)),
                ("fps".to_string(), ConfigVal::Int(60)),
                ("vsync".to_string(), ConfigVal::Bool(true)),
                ("show_playtime".to_string(), ConfigVal::Bool(true)),
                ("sort_by".to_string(), ConfigVal::Str("name".to_string())),
            ],
        ),
        (
            "art".to_string(),
            vec![
                ("sgdb_api_key".to_string(), ConfigVal::Str(String::new())),
                (
                    "sgdb_style".to_string(),
                    ConfigVal::Str("white_logo".to_string()),
                ),
                (
                    "sgdb_dimensions".to_string(),
                    ConfigVal::Str("600x900".to_string()),
                ),
                ("prefer_animated".to_string(), ConfigVal::Bool(false)),
                ("card_width".to_string(), ConfigVal::Int(190)),
            ],
        ),
        (
            "igdb".to_string(),
            vec![
                ("client_id".to_string(), ConfigVal::Str(String::new())),
                ("client_secret".to_string(), ConfigVal::Str(String::new())),
            ],
        ),
        (
            "ui".to_string(),
            vec![
                ("grid_cols".to_string(), ConfigVal::Int(4)),
                ("grid_gap".to_string(), ConfigVal::Int(30)),
                ("sidebar_width".to_string(), ConfigVal::Int(300)),
                ("card_border_width".to_string(), ConfigVal::Int(2)),
                ("scroll_speed".to_string(), ConfigVal::Int(8)),
                ("animation_speed".to_string(), ConfigVal::Float(12.0)),
                ("pixel_shift".to_string(), ConfigVal::Bool(false)),
                ("animations".to_string(), ConfigVal::Bool(false)),
            ],
        ),
    ]
}

fn set_in_sections(sections: &mut SectionMap, sec: &str, key: &str, val: ConfigVal) {
    for (s_name, keys) in sections.iter_mut() {
        if s_name == sec {
            for (k_name, k_val) in keys.iter_mut() {
                if k_name == key {
                    *k_val = val;
                    return;
                }
            }
            keys.push((key.to_string(), val));
            return;
        }
    }
    sections.push((sec.to_string(), vec![(key.to_string(), val)]));
}

fn apply_toml_file(sections: &mut SectionMap, path: &PathBuf) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    let Ok(table) = content.parse::<toml::Table>() else {
        eprintln!("[config] Warning: could not parse {:?}", path);
        return;
    };
    for (sec, val) in table {
        if let Some(sub) = val.as_table() {
            for (k, v) in sub {
                let cval = match v {
                    toml::Value::Boolean(b) => Some(ConfigVal::Bool(*b)),
                    toml::Value::Integer(i) => Some(ConfigVal::Int(*i)),
                    toml::Value::Float(f) => Some(ConfigVal::Float(*f)),
                    toml::Value::String(s) => Some(ConfigVal::Str(s.clone())),
                    _ => None,
                };
                if let Some(cv) = cval {
                    set_in_sections(sections, &sec, k, cv);
                }
            }
        }
    }
}

static CFG: LazyLock<RwLock<SectionMap>> = LazyLock::new(|| {
    let _ = fs::create_dir_all(config_dir());
    let _ = fs::create_dir_all(cache_dir());
    let _ = fs::create_dir_all(art_cache_dir());
    let _ = fs::create_dir_all(custom_art_dir());
    let _ = fs::File::create(config_dir().join(".trackerignore"));
    let _ = fs::File::create(cache_dir().join(".trackerignore"));

    let mut sections = default_sections();
    let cf = config_file();
    if cf.exists() {
        apply_toml_file(&mut sections, &cf);
    }
    let local = PathBuf::from("config.toml");
    if local.exists() {
        if let (Ok(p1), Ok(p2)) = (local.canonicalize(), cf.canonicalize()) {
            if p1 != p2 {
                apply_toml_file(&mut sections, &local);
            }
        } else {
            apply_toml_file(&mut sections, &local);
        }
    }
    RwLock::new(sections)
});

pub fn init() {
    let _ = &*CFG;
}

pub fn get_val(section: &str, key: &str) -> Option<ConfigVal> {
    let guard = CFG.read().ok()?;
    for (s, keys) in guard.iter() {
        if s == section {
            for (k, v) in keys.iter() {
                if k == key {
                    return Some(v.clone());
                }
            }
        }
    }
    None
}

pub fn get_bool(section: &str, key: &str) -> bool {
    get_val(section, key).and_then(|v| v.as_bool()).unwrap_or(false)
}

pub fn get_int(section: &str, key: &str) -> i64 {
    get_val(section, key).and_then(|v| v.as_int()).unwrap_or(0)
}

pub fn get_float(section: &str, key: &str) -> f64 {
    get_val(section, key).and_then(|v| v.as_float()).unwrap_or(0.0)
}

pub fn get_str(section: &str, key: &str) -> String {
    get_val(section, key)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default()
}

pub fn set_val(section: &str, key: &str, val: ConfigVal) {
    if let Ok(mut guard) = CFG.write() {
        set_in_sections(&mut guard, section, key, val);
    }
    save_config();
}

pub fn save_config() {
    let Ok(guard) = CFG.read() else {
        return;
    };
    let mut lines = Vec::new();
    for (section, keys) in guard.iter() {
        lines.push(format!("[{}]", section));
        for (key, val) in keys.iter() {
            let val_str = match val {
                ConfigVal::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
                ConfigVal::Int(i) => i.to_string(),
                ConfigVal::Float(f) => {
                    if f.fract() == 0.0 {
                        format!("{:.1}", f)
                    } else {
                        f.to_string()
                    }
                }
                ConfigVal::Str(s) => format!("\"{}\"", s),
            };
            lines.push(format!("{} = {}", key, val_str));
        }
        lines.push(String::new());
    }
    let cf = config_file();
    if let Err(e) = fs::write(&cf, lines.join("\n")) {
        eprintln!("[config] Error saving config to {:?}: {}", cf, e);
    }
}

pub fn sgdb_api_key() -> String {
    get_str("art", "sgdb_api_key")
}

pub fn window_size() -> (u32, u32) {
    let w = get_int("general", "window_width").max(640) as u32;
    let h = get_int("general", "window_height").max(480) as u32;
    (w, h)
}

pub fn grid_gap() -> i32 {
    get_int("ui", "grid_gap") as i32
}

pub fn igdb_credentials() -> (String, String) {
    (
        get_str("igdb", "client_id"),
        get_str("igdb", "client_secret"),
    )
}

pub fn load_art_selections() -> HashMap<String, String> {
    let p = config_dir().join("art_selections.json");
    if let Ok(txt) = fs::read_to_string(p) {
        if let Ok(map) = serde_json::from_str(&txt) {
            return map;
        }
    }
    HashMap::new()
}

pub fn save_art_selections(map: &HashMap<String, String>) {
    let p = config_dir().join("art_selections.json");
    if let Ok(txt) = serde_json::to_string_pretty(map) {
        let _ = fs::write(p, txt);
    }
}
