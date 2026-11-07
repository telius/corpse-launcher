use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::thread;

use sdl2::image::LoadSurface;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::surface::Surface;
use sdl2::video::{Window, WindowContext};

use crate::config;
use crate::game::Game;
use crate::igdb;
use crate::sgdb;
use crate::ui::{wrap_text, FontCache};

pub struct OwnedTexture(pub Texture);

impl Drop for OwnedTexture {
    fn drop(&mut self) {
        unsafe {
            let ptr = self.0.raw();
            if !ptr.is_null() {
                sdl2::sys::SDL_DestroyTexture(ptr);
            }
        }
    }
}

static FETCHING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static SGDB_FAILED: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static FETCHED_PATHS: LazyLock<Mutex<HashMap<String, PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static INVALIDATED_SLUGS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub fn get_custom_art_path(slug: &str) -> Option<PathBuf> {
    let dir = config::custom_art_dir();
    for ext in [".png", ".jpg", ".jpeg", ".webp"] {
        let p = dir.join(format!("{}{}", slug, ext));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

pub fn has_custom_art(slug: &str) -> bool {
    get_custom_art_path(slug).is_some()
}

pub fn remove_custom_art(slug: &str) -> bool {
    let dir = config::custom_art_dir();
    let mut removed = false;
    for ext in [".png", ".jpg", ".jpeg", ".webp"] {
        let p = dir.join(format!("{}{}", slug, ext));
        if p.exists() && fs::remove_file(&p).is_ok() {
            removed = true;
        }
    }
    if removed {
        invalidate(slug);
    }
    removed
}

pub fn invalidate(slug: &str) {
    if let Ok(mut inv) = INVALIDATED_SLUGS.lock() {
        inv.insert(slug.to_string());
    }
}

pub fn has_pending_updates() -> bool {
    let inv_non_empty = INVALIDATED_SLUGS
        .lock()
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    inv_non_empty
}

pub fn swap_to_next_art(game: &Game) -> bool {
    if has_custom_art(&game.slug) {
        println!(
            "[art] Blocked swapping art for {}: custom art is selected",
            game.name
        );
        return false;
    }
    if !sgdb::has_key() {
        return false;
    }
    let Some(sgdb_id) = sgdb::get_sgdb_id(&game.name, game.steam_appid.as_deref()) else {
        println!("[art] Could not resolve SGDB ID for {}", game.name);
        return false;
    };

    let style = config::get_str("art", "sgdb_style");
    let dimensions = config::get_str("art", "sgdb_dimensions");
    let urls = sgdb::fetch_all_grid_urls(sgdb_id, &style, &dimensions);
    if urls.is_empty() {
        println!(
            "[art] No grids found matching style={} dimensions={} for {}",
            style, dimensions, game.name
        );
        return false;
    }

    let mut selections = config::load_art_selections();
    let current_url = selections.get(&game.slug).cloned().unwrap_or_default();
    let idx = urls.iter().position(|u| *u == current_url).unwrap_or(0);
    let next_idx = (idx + 1) % urls.len();
    let next_url = &urls[next_idx];

    let dest = sgdb::cache_path(&game.slug);
    println!(
        "[art] Swapping art for {} from index {} to {} (total {})",
        game.name,
        idx,
        next_idx,
        urls.len()
    );
    if sgdb::download_url_to_path(next_url, &dest) {
        selections.insert(game.slug.clone(), next_url.clone());
        config::save_art_selections(&selections);
        if let Ok(mut fp) = FETCHED_PATHS.lock() {
            fp.insert(game.slug.clone(), dest);
        }
        invalidate(&game.slug);
        return true;
    }
    false
}

fn spawn_background_fetch(game: &Game) {
    let slug = game.slug.clone();
    let name = game.name.clone();
    let appid = game.steam_appid.clone();

    let should_spawn = {
        let Ok(mut fetching) = FETCHING.lock() else {
            return;
        };
        let Ok(failed) = SGDB_FAILED.lock() else {
            return;
        };
        if fetching.contains(&slug) || failed.contains(&slug) || !sgdb::has_key() {
            false
        } else {
            fetching.insert(slug.clone());
            true
        }
    };

    if !should_spawn {
        return;
    }

    thread::spawn(move || {
        let _ = igdb::fetch_metadata(&name, &slug);
        println!("[art] Fetching SGDB art: {:?}", name);
        let path = sgdb::fetch_art(&slug, appid.as_deref(), &name);

        if let Ok(mut fetching) = FETCHING.lock() {
            fetching.remove(&slug);
        }
        match path {
            Some(p) if p.exists() => {
                println!("[art] SGDB done: {:?} → {:?}", name, p.file_name().unwrap_or_default());
                if let Ok(mut fp) = FETCHED_PATHS.lock() {
                    fp.insert(slug.clone(), p);
                }
                invalidate(&slug);
            }
            _ => {
                if let Ok(mut failed) = SGDB_FAILED.lock() {
                    failed.insert(slug);
                }
                println!("[art] SGDB no result: {:?}", name);
            }
        }
    });
}

pub struct ArtCache {
    textures: HashMap<String, OwnedTexture>,
}

impl ArtCache {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.textures.clear();
        if let Ok(mut f) = FETCHING.lock() {
            f.clear();
        }
        if let Ok(mut s) = SGDB_FAILED.lock() {
            s.clear();
        }
    }

    pub fn process_invalidations(&mut self) -> bool {
        let Ok(mut inv) = INVALIDATED_SLUGS.lock() else {
            return false;
        };
        if inv.is_empty() {
            return false;
        }
        for slug in inv.drain() {
            self.textures.remove(&slug);
        }
        true
    }

    pub fn get_or_load_texture(
        &mut self,
        tc: &TextureCreator<WindowContext>,
        game: &Game,
    ) -> Option<&mut Texture> {
        self.process_invalidations();

        if self.textures.contains_key(&game.slug) {
            return self.textures.get_mut(&game.slug).map(|t| &mut t.0);
        }

        // Resolve image path:
        // 1. Custom art override
        // 2. Newly fetched SGDB path
        // 3. game.art_path
        // 4. Cached SGDB file on disk
        let path = get_custom_art_path(&game.slug)
            .or_else(|| {
                FETCHED_PATHS
                    .lock()
                    .ok()
                    .and_then(|m| m.get(&game.slug).cloned())
            })
            .or_else(|| game.art_path.clone().filter(|p| p.exists()))
            .or_else(|| {
                let p = sgdb::cache_path(&game.slug);
                if p.exists() {
                    Some(p)
                } else {
                    None
                }
            });

        if let Some(p) = path {
            if let Ok(surf) = Surface::from_file(&p) {
                if let Ok(mut tex) = tc.create_texture_from_surface(&surf) {
                    tex.set_blend_mode(BlendMode::Blend);
                    self.textures.insert(game.slug.clone(), OwnedTexture(tex));
                    return self.textures.get_mut(&game.slug).map(|t| &mut t.0);
                }
            }
        }

        // Trigger SGDB background fetch
        spawn_background_fetch(game);
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_placeholder<'ttf>(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    fonts: &mut FontCache<'ttf>,
    title_pt: u16,
    badge_pt: u16,
    game: &Game,
    rect: Rect,
    alpha_mod: u8,
) {
    let scale = (alpha_mod as f32) / 255.0;
    canvas.set_blend_mode(BlendMode::Blend);

    // Card background
    canvas.set_draw_color(Color::RGBA(
        config::BG_CARD.0,
        config::BG_CARD.1,
        config::BG_CARD.2,
        alpha_mod,
    ));
    let _ = canvas.fill_rect(rect);

    // Top half violet gradient
    let half_h = (rect.height() / 2).max(1) as i32;
    for y in 0..half_h {
        let a = ((90.0 * (1.0 - (y as f32) / (half_h as f32))) * scale) as u8;
        canvas.set_draw_color(Color::RGBA(
            config::VIOLET.0,
            config::VIOLET.1,
            config::VIOLET.2,
            a,
        ));
        let _ = canvas.draw_line(
            (rect.x(), rect.y() + y),
            (rect.x() + rect.width() as i32 - 1, rect.y() + y),
        );
    }

    // Border
    canvas.set_draw_color(Color::RGBA(
        config::BORDER_BG.0,
        config::BORDER_BG.1,
        config::BORDER_BG.2,
        alpha_mod,
    ));
    let _ = canvas.draw_rect(rect);

    // Diagonal accent line
    canvas.set_draw_color(Color::RGBA(
        config::VIOLET.0,
        config::VIOLET.1,
        config::VIOLET.2,
        (40.0 * scale) as u8,
    ));
    let _ = canvas.draw_line(
        (rect.x(), rect.y()),
        (
            rect.x() + rect.width() as i32 - 1,
            rect.y() + rect.height() as i32 - 1,
        ),
    );

    // Wrapped title text
    let max_w = (rect.width() as i32 - 16).max(20) as u32;
    let lines = {
        let tf = fonts.get_font(title_pt, true);
        wrap_text(&game.name, tf, max_w)
    };
    let mut text_y = rect.y() + (rect.height() as i32) / 2;
    for line in lines.iter().take(4) {
        if let Some((_, h)) = fonts.draw_text(
            canvas,
            tc,
            title_pt,
            true,
            line,
            Color::RGBA(
                config::WHITE.0,
                config::WHITE.1,
                config::WHITE.2,
                alpha_mod,
            ),
            rect.x() + (rect.width() as i32) / 2,
            text_y,
            true,
        ) {
            text_y += h as i32 + 3;
        }
    }

    // Platform badge at bottom
    let label = game.display_platform_label().to_uppercase();
    if let Ok((tw, th)) = fonts.size_of(badge_pt, false, &label) {
        let _ = fonts.draw_text(
            canvas,
            tc,
            badge_pt,
            false,
            &label,
            Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, alpha_mod),
            rect.x() + (rect.width() as i32 - tw as i32) / 2,
            rect.y() + rect.height() as i32 - th as i32 - 8,
            false,
        );
    }
}
