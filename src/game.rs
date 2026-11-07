use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Steam,
    Lutris,
    Battlenet,
    Gog,
    Epic,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Steam => "steam",
            Platform::Lutris => "lutris",
            Platform::Battlenet => "battlenet",
            Platform::Gog => "gog",
            Platform::Epic => "epic",
        }
    }

    pub fn display_label(&self) -> &'static str {
        match self {
            Platform::Steam => "Steam",
            Platform::Lutris => "Lutris",
            Platform::Battlenet => "Battle.net",
            Platform::Gog => "GOG",
            Platform::Epic => "Epic",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Game {
    pub slug: String,
    pub name: String,
    pub platform: Platform,

    // Steam-specific
    pub steam_appid: Option<String>,
    pub is_shortcut: bool,

    // Lutris-specific
    pub lutris_id: Option<i64>,
    pub lutris_slug: Option<String>,
    pub runner: String,

    // Art
    pub art_path: Option<PathBuf>,

    // Metadata
    pub install_dir: Option<PathBuf>,
    pub playtime_minutes: i64,
    pub last_played: i64,
    pub year: Option<i64>,

    // Launch override
    pub launch_via_lutris: bool,
    pub lutris_launch_id: Option<i64>,

    // Dedup priority
    pub mtime: f64,
}

impl Game {
    pub fn display_platform_label(&self) -> &'static str {
        self.platform.display_label()
    }

    pub fn playtime_str(&self) -> String {
        if matches!(self.platform, Platform::Lutris | Platform::Battlenet)
            || self.playtime_minutes <= 0
        {
            return String::new();
        }
        let h = self.playtime_minutes / 60;
        let m = self.playtime_minutes % 60;
        if h > 0 {
            format!("{}h {}m", h, m)
        } else {
            format!("{}m", m)
        }
    }
}
