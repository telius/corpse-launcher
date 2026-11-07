use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use sdl2::image::LoadSurface;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::surface::Surface;
use sdl2::ttf::{Font, Sdl2TtfContext};
use sdl2::video::{Window, WindowContext};

use crate::art::{self, ArtCache, OwnedTexture};
use crate::config::{self, ConfigVal};
use crate::game::{Game, Platform};
use crate::hidden;
use crate::igdb;

const REGULAR_FONTS: &[&str] = &[
    "/usr/share/fonts/gnu-free/FreeSans.otf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

const BOLD_FONTS: &[&str] = &[
    "/usr/share/fonts/gnu-free/FreeSansBold.otf",
    "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
];

fn resolve_font_path(candidates: &[&'static str]) -> &'static str {
    for &path in candidates {
        if Path::new(path).exists() {
            return path;
        }
    }
    for &path in REGULAR_FONTS {
        if Path::new(path).exists() {
            return path;
        }
    }
    panic!("No usable system TrueType/OpenType font found");
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextCacheKey {
    text: String,
    size: u16,
    bold: bool,
    r: u8,
    g: u8,
    b: u8,
}

pub struct FontCache<'ttf> {
    ttf: &'ttf Sdl2TtfContext,
    reg_path: &'static str,
    bold_path: &'static str,
    fonts: HashMap<(u16, bool), Font<'ttf, 'static>>,
    text_textures: HashMap<TextCacheKey, (OwnedTexture, u32, u32)>,
}

impl<'ttf> FontCache<'ttf> {
    pub fn new(ttf: &'ttf Sdl2TtfContext) -> Self {
        Self {
            ttf,
            reg_path: resolve_font_path(REGULAR_FONTS),
            bold_path: resolve_font_path(BOLD_FONTS),
            fonts: HashMap::new(),
            text_textures: HashMap::new(),
        }
    }

    pub fn clear_textures(&mut self) {
        self.text_textures.clear();
    }

    pub fn get_font(&mut self, size: u16, bold: bool) -> &Font<'ttf, 'static> {
        let key = (size, bold);
        if !self.fonts.contains_key(&key) {
            let path = if bold { self.bold_path } else { self.reg_path };
            let font = self
                .ttf
                .load_font(path, size)
                .expect("Failed to load resolved system font");
            self.fonts.insert(key, font);
        }
        self.fonts.get(&key).unwrap()
    }

    pub fn height(&mut self, size: u16, bold: bool) -> i32 {
        self.get_font(size, bold).height()
    }

    pub fn size_of(&mut self, size: u16, bold: bool, text: &str) -> Result<(u32, u32), String> {
        self.get_font(size, bold)
            .size_of(text)
            .map_err(|e| e.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        size: u16,
        bold: bool,
        text: &str,
        color: Color,
        x: i32,
        y: i32,
        center_x: bool,
    ) -> Option<(u32, u32)> {
        if text.is_empty() {
            return Some((0, self.height(size, bold).max(1) as u32));
        }

        if self.text_textures.len() > 512 {
            self.text_textures.clear();
        }

        let key = TextCacheKey {
            text: text.to_string(),
            size,
            bold,
            r: color.r,
            g: color.g,
            b: color.b,
        };

        if !self.text_textures.contains_key(&key) {
            let font = self.get_font(size, bold);
            let surf = font
                .render(text)
                .blended(Color::RGB(color.r, color.g, color.b))
                .ok()?;
            let w = surf.width();
            let h = surf.height();
            let mut tex = tc.create_texture_from_surface(&surf).ok()?;
            tex.set_blend_mode(BlendMode::Blend);
            self.text_textures.insert(key.clone(), (OwnedTexture(tex), w, h));
        }

        let (owned, w, h) = self.text_textures.get_mut(&key)?;
        owned.0.set_alpha_mod(color.a);
        let draw_x = if center_x { x - (*w as i32) / 2 } else { x };
        let _ = canvas.copy(&owned.0, None, Some(Rect::new(draw_x, y, *w, *h)));
        owned.0.set_alpha_mod(255);
        Some((*w, *h))
    }
}

// ---------------------------------------------------------------------------
// Easing & Tween
// ---------------------------------------------------------------------------

pub fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

pub fn lerp(current: f64, target: f64, speed: f64, dt: f64) -> f64 {
    if !config::get_bool("ui", "animations") {
        return target;
    }
    let diff = target - current;
    if diff.abs() < 0.5 {
        return target;
    }
    current + diff * (speed * dt).min(1.0)
}

#[derive(Clone, Debug)]
pub struct Tween {
    start: f64,
    end: f64,
    duration: f64,
    elapsed: f64,
    pub value: f64,
    pub done: bool,
}

impl Tween {
    pub fn new(start: f64, end: f64, duration: f64) -> Self {
        Self {
            start,
            end,
            duration: duration.max(0.001),
            elapsed: 0.0,
            value: start,
            done: false,
        }
    }

    pub fn update(&mut self, dt: f64) {
        if self.done {
            return;
        }
        if !config::get_bool("ui", "animations") {
            self.value = self.end;
            self.done = true;
            return;
        }
        self.elapsed += dt;
        let t = (self.elapsed / self.duration).min(1.0);
        self.value = self.start + (self.end - self.start) * ease_out_cubic(t);
        if t >= 1.0 {
            self.value = self.end;
            self.done = true;
        }
    }

    pub fn reset(&mut self, start: f64, end: f64, duration: Option<f64>) {
        self.start = start;
        self.end = end;
        if let Some(d) = duration {
            self.duration = d.max(0.001);
        }
        self.elapsed = 0.0;
        self.value = start;
        self.done = false;
    }
}

// ---------------------------------------------------------------------------
// Text & Drawing Helpers
// ---------------------------------------------------------------------------

pub fn wrap_text(text: &str, font: &Font, max_w: u32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in text.split_whitespace() {
        let test = if cur.is_empty() {
            w.to_string()
        } else {
            format!("{} {}", cur, w)
        };
        let fits = font.size_of(&test).map(|(tw, _)| tw <= max_w).unwrap_or(true);
        if fits {
            cur = test;
        } else {
            if !cur.is_empty() {
                lines.push(cur);
            }
            cur = w.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn colour_for_platform(p: Platform) -> (u8, u8, u8) {
    match p {
        Platform::Steam => config::BADGE_STEAM,
        Platform::Lutris => config::BADGE_LUTRIS,
        Platform::Battlenet => config::BADGE_BN,
        _ => config::GREY,
    }
}

// ---------------------------------------------------------------------------
// PS5 Controller Icon Cache
// ---------------------------------------------------------------------------

const ICON_DIR: &str =
    "/home/james/projects/PS5 Button Icons and Controls/Buttons Outline/White/256w";

fn icon_filename(name: &str) -> Option<&'static str> {
    match name {
        "cross" => Some("Cross.png"),
        "circle" => Some("Circle.png"),
        "triangle" => Some("Triangle.png"),
        "square" => Some("Square.png"),
        "l1" => Some("L1.png"),
        "r1" => Some("R1.png"),
        "l2" => Some("L2.png"),
        "r2" => Some("R2.png"),
        "options" => Some("Options.png"),
        "create" => Some("Create.png"),
        "touchpad" => Some("Touch Pad Press.png"),
        "dpad" => Some("D-Pad.png"),
        "dpad_up" => Some("D-Pad Up.png"),
        "dpad_down" => Some("D-Pad Down.png"),
        "dpad_left" => Some("D-Pad Left.png"),
        "dpad_right" => Some("D-Pad Right.png"),
        "lstick" => Some("Left Stick.png"),
        _ => None,
    }
}

pub struct IconCache {
    textures: HashMap<String, OwnedTexture>,
}

impl IconCache {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }

    pub fn get(&mut self, tc: &TextureCreator<WindowContext>, name: &str) -> Option<&mut Texture> {
        if self.textures.contains_key(name) {
            return self.textures.get_mut(name).map(|t| &mut t.0);
        }
        let fname = icon_filename(name)?;
        let path = Path::new(ICON_DIR).join(fname);
        if !path.exists() {
            return None;
        }
        let surf = Surface::from_file(&path).ok()?;
        let mut tex = tc.create_texture_from_surface(&surf).ok()?;
        tex.set_blend_mode(BlendMode::Blend);
        self.textures.insert(name.to_string(), OwnedTexture(tex));
        self.textures.get_mut(name).map(|t| &mut t.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_hint<'ttf>(
        &mut self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        fonts: &mut FontCache<'ttf>,
        x: i32,
        y: i32,
        icon_name: &str,
        label: &str,
        font_pt: u16,
        font_bold: bool,
        icon_size: u32,
        colour: Color,
        gap: i32,
    ) -> i32 {
        let mut cx = x;
        let fh = fonts.height(font_pt, font_bold);
        if let Some(tex) = self.get(tc, icon_name) {
            let iy = y + (fh - icon_size as i32) / 2;
            tex.set_alpha_mod(colour.a);
            let _ = canvas.copy(tex, None, Some(Rect::new(cx, iy, icon_size, icon_size)));
            tex.set_alpha_mod(255);
            cx += icon_size as i32 + gap;
        }
        if let Ok((tw, th)) = fonts.size_of(font_pt, font_bold, label) {
            let ty = y + (fh - th as i32) / 2 + 1;
            let _ = fonts.draw_text(canvas, tc, font_pt, font_bold, label, colour, cx, ty, false);
            cx += tw as i32;
        }
        cx
    }
}

// ---------------------------------------------------------------------------
// Grid
// ---------------------------------------------------------------------------

pub struct Grid {
    pub games: Vec<Game>,
    pub rect: Rect,
    pub selected: usize,
    pub scroll_y: f64,
    pub target_y: f64,
    pub dirty: bool,
    anim_speed: f64,
}

impl Grid {
    const TARGET_CARD_W: i32 = 190;
    const MIN_COLS: i32 = 2;
    const MAX_COLS: i32 = 8;

    pub fn new(games: Vec<Game>, rect: Rect) -> Self {
        Self {
            games,
            rect,
            selected: 0,
            scroll_y: 0.0,
            target_y: 0.0,
            dirty: true,
            anim_speed: config::get_float("ui", "animation_speed"),
        }
    }

    pub fn gap(&self) -> i32 {
        config::grid_gap()
    }

    pub fn cols(&self) -> i32 {
        let gap = self.gap();
        let avail = (self.rect.width() as i32) - gap;
        let c = avail / (Self::TARGET_CARD_W + gap);
        c.clamp(Self::MIN_COLS, Self::MAX_COLS)
    }

    pub fn card_w(&self) -> i32 {
        let cols = self.cols();
        let gap = self.gap();
        let avail = (self.rect.width() as i32) - gap * (cols + 1);
        (avail / cols).max(80)
    }

    pub fn card_h(&self) -> i32 {
        self.card_w() * 900 / 600
    }

    pub fn rows(&self) -> i32 {
        let cols = self.cols();
        let total = self.games.len() as i32;
        ((total + cols - 1) / cols).max(1)
    }

    pub fn total_height(&self) -> i32 {
        let gap = self.gap();
        self.rows() * (self.card_h() + gap) + gap
    }

    fn h_offset(&self) -> i32 {
        let cols = self.cols();
        let gap = self.gap();
        let used = cols * self.card_w() + (cols + 1) * gap;
        (((self.rect.width() as i32) - used) / 2).max(0)
    }

    fn v_offset(&self) -> i32 {
        let th = self.total_height();
        let rh = self.rect.height() as i32;
        if th < rh {
            ((rh - th) / 2).max(0)
        } else {
            0
        }
    }

    fn row_offset(&self, row: i32) -> i32 {
        let total = self.games.len() as i32;
        if total <= 0 {
            return 0;
        }
        let cols = self.cols();
        let last_row = (total - 1) / cols;
        let rem = total % cols;
        if row == last_row && rem != 0 {
            ((cols - rem) * (self.card_w() + self.gap())) / 2
        } else {
            0
        }
    }

    fn card_rect(&self, index: usize) -> Rect {
        let cols = self.cols();
        let gap = self.gap();
        let cw = self.card_w();
        let ch = self.card_h();
        let idx = index as i32;
        let row = idx / cols;
        let col = idx % cols;
        let x = self.rect.x() + self.h_offset() + gap + self.row_offset(row) + col * (cw + gap);
        let y = self.v_offset() + gap + row * (ch + gap);
        Rect::new(x, y, cw as u32, ch as u32)
    }

    pub fn move_sel(&mut self, dx: i32, dy: i32) {
        let total = self.games.len() as i32;
        if total <= 0 {
            return;
        }
        let cols = self.cols();
        let sel = self.selected as i32;
        let cur_row = sel / cols;
        let cur_col = sel % cols;

        let new_idx = if dx != 0 {
            let row_start = cur_row * cols;
            let row_end = total.min(row_start + cols);
            let row_len = (row_end - row_start).max(1);
            let col_in_row = sel - row_start;
            let new_col = (col_in_row + dx).rem_euclid(row_len);
            row_start + new_col
        } else if dy != 0 {
            let matching: Vec<i32> = (0..total).filter(|i| (i % cols) == cur_col).collect();
            if let Some(pos) = matching.iter().position(|&i| i == sel) {
                let next_pos = (pos as i32 + dy).rem_euclid(matching.len() as i32) as usize;
                matching[next_pos]
            } else {
                sel
            }
        } else {
            sel
        };

        if new_idx as usize != self.selected {
            self.selected = new_idx as usize;
            self.ensure_visible();
            self.dirty = true;
        }
    }

    pub fn page(&mut self, direction: i32) {
        if self.games.is_empty() {
            return;
        }
        let old = self.selected;
        let visible_rows = ((self.rect.height() as i32) / (self.card_h() + self.gap())).max(1);
        let target = (self.selected as i32 + direction * self.cols() * visible_rows)
            .clamp(0, self.games.len() as i32 - 1);
        self.selected = target as usize;
        if self.selected != old {
            self.ensure_visible();
            self.dirty = true;
        }
    }

    pub fn scroll_by(&mut self, amount: f64) {
        self.target_y += amount;
        let max_scroll = ((self.total_height() - self.rect.height() as i32).max(0)) as f64;
        self.target_y = self.target_y.clamp(0.0, max_scroll);
        self.dirty = true;
    }

    pub fn hover_at(&self, mx: i32, my: i32) -> Option<usize> {
        if !self.rect.contains_point((mx, my)) {
            return None;
        }
        let total = self.games.len() as i32;
        if total == 0 {
            return None;
        }
        let gap = self.gap();
        let cols = self.cols();
        let col_w = self.card_w() + gap;
        let row_h = self.card_h() + gap;

        let rel_y = my - self.rect.y() - self.v_offset() - gap + (self.scroll_y as i32);
        if rel_y < 0 {
            return None;
        }
        let row = rel_y / row_h;
        if row < 0 || row >= self.rows() {
            return None;
        }

        let row_off = self.row_offset(row);
        let rel_x = mx - self.rect.x() - self.h_offset() - gap - row_off;
        if rel_x < 0 {
            return None;
        }
        let col = rel_x / col_w;

        let last_row = (total - 1) / cols;
        let rem = total % cols;
        let max_col = if row == last_row && rem != 0 { rem } else { cols };
        if col < 0 || col >= max_col {
            return None;
        }

        let idx = (row * cols + col) as usize;
        if idx >= self.games.len() {
            return None;
        }

        let mut cr = self.card_rect(idx);
        let sy = cr.y() - (self.scroll_y as i32) + self.rect.y();
        cr.set_y(sy);
        if cr.contains_point((mx, my)) {
            Some(idx)
        } else {
            None
        }
    }

    fn ensure_visible(&mut self) {
        if self.games.is_empty() {
            self.target_y = 0.0;
            return;
        }
        let r = self.card_rect(self.selected);
        let gap = self.gap() as f64;
        let ry = r.y() as f64;
        let ch = self.card_h() as f64;
        let rh = self.rect.height() as f64;

        if ry - self.target_y < gap {
            self.target_y = ry - gap;
        }
        if ry + ch - self.target_y > rh - gap {
            self.target_y = ry + ch - rh + gap;
        }
        let max_scroll = ((self.total_height() - self.rect.height() as i32).max(0)) as f64;
        self.target_y = self.target_y.clamp(0.0, max_scroll);
    }

    pub fn set_games(&mut self, games: Vec<Game>) {
        let prev_slug = self.games.get(self.selected).map(|g| g.slug.clone());

        self.games = games;
        let mut new_idx = 0;
        if let Some(slug) = prev_slug {
            if let Some(pos) = self.games.iter().position(|g| g.slug == slug) {
                new_idx = pos;
            }
        }
        if self.games.is_empty() {
            self.selected = 0;
        } else {
            self.selected = new_idx.min(self.games.len() - 1);
        }
        self.ensure_visible();
        self.dirty = true;
    }

    pub fn resize(&mut self, rect: Rect) {
        self.rect = rect;
        self.ensure_visible();
        self.dirty = true;
    }

    pub fn selected_game(&self) -> Option<&Game> {
        self.games.get(self.selected)
    }

    pub fn update(&mut self, dt: f64) -> bool {
        let old_scroll = self.scroll_y;
        self.scroll_y = lerp(self.scroll_y, self.target_y, self.anim_speed, dt);
        if (self.scroll_y - old_scroll).abs() > 0.5 {
            self.dirty = true;
        }
        self.dirty
    }

    pub fn draw<'ttf>(
        &mut self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        fonts: &mut FontCache<'ttf>,
        art_cache: &mut ArtCache,
        shift_x: i32,
        shift_y: i32,
    ) {
        if self.games.is_empty() {
            let _ = fonts.draw_text(
                canvas,
                tc,
                24,
                false,
                "No games found — loading library...",
                Color::RGB(config::GREY.0, config::GREY.1, config::GREY.2),
                self.rect.x() + (self.rect.width() as i32) / 2 + shift_x,
                self.rect.y() + (self.rect.height() as i32) / 2 - 12 + shift_y,
                true,
            );
            self.dirty = false;
            return;
        }

        let cw = self.card_w();
        let ch = self.card_h();
        let scroll = self.scroll_y as i32;

        let title_pt = (cw / 14).max(10) as u16;
        let badge_pt = (cw / 18).max(9) as u16;

        let clip = Rect::new(
            self.rect.x() + 1 + shift_x,
            self.rect.y() + 1 + shift_y,
            (self.rect.width() as i32 - 2).max(1) as u32,
            (self.rect.height() as i32 - 2).max(1) as u32,
        );
        canvas.set_clip_rect(Some(clip));

        let view_top = self.rect.y() + shift_y;
        let view_bottom = view_top + (self.rect.height() as i32);

        // Pass 1: draw visible unselected cards
        for (idx, game) in self.games.iter().enumerate() {
            if idx == self.selected {
                continue;
            }
            let local = self.card_rect(idx);
            let sx = local.x() + shift_x;
            let sy = local.y() - scroll + self.rect.y() + shift_y;

            if sy + ch < view_top {
                continue;
            }
            if sy > view_bottom {
                break;
            }

            Self::draw_card(
                canvas,
                tc,
                fonts,
                title_pt,
                badge_pt,
                art_cache,
                game,
                Rect::new(sx, sy, cw as u32, ch as u32),
                false,
            );
        }

        // Pass 2: draw selected card on top so its 1.06x zoom and glow are never overlapped
        if let Some(sel_game) = self.games.get(self.selected) {
            let local = self.card_rect(self.selected);
            let sx = local.x() + shift_x;
            let sy = local.y() - scroll + self.rect.y() + shift_y;
            if sy + ch >= view_top && sy <= view_bottom {
                Self::draw_card(
                    canvas,
                    tc,
                    fonts,
                    title_pt,
                    badge_pt,
                    art_cache,
                    sel_game,
                    Rect::new(sx, sy, cw as u32, ch as u32),
                    true,
                );
            }
        }

        canvas.set_clip_rect(None);
        self.dirty = false;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_card<'ttf>(
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        fonts: &mut FontCache<'ttf>,
        title_pt: u16,
        badge_pt: u16,
        art_cache: &mut ArtCache,
        game: &Game,
        rect: Rect,
        selected: bool,
    ) {
        let cw = rect.width() as i32;
        let ch = rect.height() as i32;
        canvas.set_blend_mode(BlendMode::Blend);

        let (draw_rect, padding) = if selected {
            let nw = ((cw as f64) * 1.06) as i32;
            let nh = ((ch as f64) * 1.06) as i32;
            let dr = Rect::new(
                rect.x() - (nw - cw) / 2,
                rect.y() - (nh - ch) / 2,
                nw as u32,
                nh as u32,
            );

            // Outer glow
            for i in [8, 6, 4, 2] {
                let gr = Rect::new(
                    dr.x() - i / 2,
                    dr.y() - i / 2,
                    (dr.width() as i32 + i) as u32,
                    (dr.height() as i32 + i) as u32,
                );
                let alpha = (45.0 * (1.0 - (i as f32) / 10.0)) as u8;
                canvas.set_draw_color(Color::RGBA(
                    config::VIOLET.0,
                    config::VIOLET.1,
                    config::VIOLET.2,
                    alpha,
                ));
                let _ = canvas.fill_rect(gr);
            }
            (dr, (nw / 24).max(4))
        } else {
            (rect, (cw / 24).max(4))
        };

        // Card container background
        canvas.set_draw_color(Color::RGB(
            config::BG_CARD.0,
            config::BG_CARD.1,
            config::BG_CARD.2,
        ));
        let _ = canvas.fill_rect(draw_rect);

        let art_rect = Rect::new(
            draw_rect.x() + padding,
            draw_rect.y() + padding,
            (draw_rect.width() as i32 - padding * 2).max(1) as u32,
            (draw_rect.height() as i32 - padding * 2).max(1) as u32,
        );

        if let Some(tex) = art_cache.get_or_load_texture(tc, game) {
            tex.set_alpha_mod(255);
            let _ = canvas.copy(tex, None, Some(art_rect));
        } else {
            art::draw_placeholder(canvas, tc, fonts, title_pt, badge_pt, game, art_rect, 255);
        }

        // Dim unselected cards
        if !selected {
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 95));
            let _ = canvas.fill_rect(draw_rect);
        }

        // Border
        if selected {
            canvas.set_draw_color(Color::RGB(config::CYAN.0, config::CYAN.1, config::CYAN.2));
        } else {
            canvas.set_draw_color(Color::RGB(
                config::BORDER_BG.0,
                config::BORDER_BG.1,
                config::BORDER_BG.2,
            ));
        }
        let _ = canvas.draw_rect(draw_rect);
    }
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

pub struct Sidebar {
    pub rect: Rect,
    game: Option<Game>,
    prev_game: Option<Game>,
    pub alpha_tw: Tween,
    alpha: f64,
    clicked_url: Option<String>,
    hovered_url: Option<String>,
    click_timer: f64,
    link_rects: Vec<(Rect, String)>,
}

impl Sidebar {
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            game: None,
            prev_game: None,
            alpha_tw: Tween::new(0.0, 255.0, 0.22),
            alpha: 0.0,
            clicked_url: None,
            hovered_url: None,
            click_timer: 0.0,
            link_rects: Vec::new(),
        }
    }

    pub fn set_game(&mut self, game: Option<Game>) {
        let same = match (&self.game, &game) {
            (Some(a), Some(b)) => a.slug == b.slug,
            (None, None) => true,
            _ => false,
        };
        if same {
            self.game = game;
            return;
        }
        if self.game.is_some() {
            self.prev_game = self.game.take();
        }
        self.game = game;
        self.alpha_tw.reset(0.0, 255.0, Some(0.20));
    }

    pub fn resize(&mut self, rect: Rect) {
        self.rect = rect;
    }

    pub fn update(&mut self, dt: f64) -> bool {
        self.alpha_tw.update(dt);
        self.alpha = self.alpha_tw.value;

        let mut timer_changed = false;
        if self.click_timer > 0.0 {
            self.click_timer = (self.click_timer - dt).max(0.0);
            timer_changed = true;
            if self.click_timer <= 0.0 {
                self.clicked_url = None;
            }
        }
        timer_changed
    }

    pub fn hover_at(&mut self, mx: i32, my: i32) -> bool {
        let new_hover = self
            .link_rects
            .iter()
            .find(|(r, _)| r.contains_point((mx, my)))
            .map(|(_, u)| u.clone());
        if new_hover != self.hovered_url {
            self.hovered_url = new_hover;
            true
        } else {
            false
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'ttf>(
        &mut self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        fonts: &mut FontCache<'ttf>,
        art_cache: &mut ArtCache,
        mouse_pos: (i32, i32),
        shift_x: i32,
        shift_y: i32,
    ) {
        let Some(ref game) = self.game else {
            return;
        };

        let w = (self.rect.width() as i32).max(1);
        let h = self.rect.height() as i32;
        let rx = self.rect.x() + shift_x;
        let ry = self.rect.y() + shift_y;

        let title_pt = (w / 12).max(14) as u16;
        let meta_pt = (w / 18).max(11) as u16;
        let hint_pt = (w / 22).max(10) as u16;
        let badge_pt = (w / 24).max(9) as u16;

        let panel_alpha = (self.alpha.clamp(0.0, 255.0) / 255.0 * 242.0) as u8;
        let text_alpha = self.alpha.clamp(0.0, 255.0) as u8;

        canvas.set_blend_mode(BlendMode::Blend);
        canvas.set_draw_color(Color::RGBA(
            config::BG_PANEL.0,
            config::BG_PANEL.1,
            config::BG_PANEL.2,
            panel_alpha,
        ));
        let _ = canvas.fill_rect(Rect::new(rx, ry, w as u32, h as u32));

        // Left separator
        canvas.set_draw_color(Color::RGBA(
            config::BORDER_BG.0,
            config::BORDER_BG.1,
            config::BORDER_BG.2,
            text_alpha,
        ));
        let _ = canvas.draw_line((rx, ry + 16), (rx, ry + h - 16));

        let mut y = ry + 12;

        // ── Art (Cross-fading) ────────────────────────────────────────────────
        let art_w = (w - 20).max(10);
        let art_h = art_w * 900 / 600;
        let art_rect = Rect::new(rx + 10, y, art_w as u32, art_h as u32);
        let blend_factor = (self.alpha / 255.0).clamp(0.0, 1.0);

        if let Some(ref prev) = self.prev_game {
            if blend_factor < 1.0 {
                let prev_alpha = ((1.0 - blend_factor) * 255.0) as u8;
                if let Some(tex) = art_cache.get_or_load_texture(tc, prev) {
                    tex.set_alpha_mod(prev_alpha);
                    let _ = canvas.copy(tex, None, Some(art_rect));
                    tex.set_alpha_mod(255);
                } else {
                    art::draw_placeholder(
                        canvas, tc, fonts, title_pt, badge_pt, prev, art_rect, prev_alpha,
                    );
                }
            }
        }

        let curr_alpha = (blend_factor * 255.0) as u8;
        if let Some(tex) = art_cache.get_or_load_texture(tc, game) {
            tex.set_alpha_mod(curr_alpha);
            let _ = canvas.copy(tex, None, Some(art_rect));
            tex.set_alpha_mod(255);
        } else {
            art::draw_placeholder(
                canvas, tc, fonts, title_pt, badge_pt, game, art_rect, curr_alpha,
            );
        }

        canvas.set_draw_color(Color::RGBA(
            config::BORDER_BG.0,
            config::BORDER_BG.1,
            config::BORDER_BG.2,
            text_alpha,
        ));
        let _ = canvas.draw_rect(art_rect);
        y += art_h + 12;

        // ── Platform badge ───────────────────────────────────────────────────
        let plat_label = game.display_platform_label().to_uppercase();
        if let Ok((tw, th)) = fonts.size_of(badge_pt, true, &plat_label) {
            let bc = colour_for_platform(game.platform);
            let bw = tw + 12;
            let bh = th + 6;
            canvas.set_draw_color(Color::RGBA(bc.0, bc.1, bc.2, text_alpha));
            let _ = canvas.fill_rect(Rect::new(rx + 10, y, bw, bh));
            let _ = fonts.draw_text(
                canvas,
                tc,
                badge_pt,
                true,
                &plat_label,
                Color::RGBA(
                    config::BG_DEEP.0,
                    config::BG_DEEP.1,
                    config::BG_DEEP.2,
                    text_alpha,
                ),
                rx + 16,
                y + 3,
                false,
            );
            y += bh as i32 + 6;
        }

        // Hidden badge
        if hidden::is_hidden(&game.slug) {
            if let Ok((tw, th)) = fonts.size_of(badge_pt, true, "HIDDEN") {
                canvas.set_draw_color(Color::RGBA(
                    config::MAGENTA.0,
                    config::MAGENTA.1,
                    config::MAGENTA.2,
                    text_alpha,
                ));
                let _ = canvas.fill_rect(Rect::new(rx + 10, y, tw + 10, th + 5));
                let _ = fonts.draw_text(
                    canvas,
                    tc,
                    badge_pt,
                    true,
                    "HIDDEN",
                    Color::RGBA(
                        config::BG_DEEP.0,
                        config::BG_DEEP.1,
                        config::BG_DEEP.2,
                        text_alpha,
                    ),
                    rx + 15,
                    y + 2,
                    false,
                );
                y += th as i32 + 8;
            }
        }

        y += 4;

        // ── Title ────────────────────────────────────────────────────────────
        let title_lines = {
            let tf = fonts.get_font(title_pt, true);
            wrap_text(&game.name, tf, (w - 16).max(20) as u32)
        };
        for line in title_lines.iter().take(3) {
            if let Some((_, th)) = fonts.draw_text(
                canvas,
                tc,
                title_pt,
                true,
                line,
                Color::RGBA(
                    config::WHITE.0,
                    config::WHITE.1,
                    config::WHITE.2,
                    text_alpha,
                ),
                rx + 10,
                y,
                false,
            ) {
                y += th as i32 + 2;
            }
        }
        y += 6;

        // ── Year & IGDB Metadata ─────────────────────────────────────────────
        let igdb_meta = igdb::get_cached_metadata(&game.slug);
        let mut display_year = game.year;
        if let Some(ref meta) = igdb_meta {
            if let Some(ry_year) = meta.release_year {
                if display_year.is_none() || display_year == Some(1996) {
                    display_year = Some(ry_year);
                }
            }
        }

        if let Some(yr) = display_year {
            if let Some((_, th)) = fonts.draw_text(
                canvas,
                tc,
                meta_pt,
                false,
                &yr.to_string(),
                Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, text_alpha),
                rx + 10,
                y,
                false,
            ) {
                y += th as i32 + 4;
            }
        }

        // ── Playtime ─────────────────────────────────────────────────────────
        if !matches!(game.platform, Platform::Lutris | Platform::Battlenet) {
            let pt = game.playtime_str();
            if !pt.is_empty() {
                if let Some((_, th)) = fonts.draw_text(
                    canvas,
                    tc,
                    meta_pt,
                    false,
                    &format!("Playtime: {}", pt),
                    Color::RGBA(config::CYAN.0, config::CYAN.1, config::CYAN.2, text_alpha),
                    rx + 10,
                    y,
                    false,
                ) {
                    y += th as i32 + 4;
                }
            }
        }

        // ── IGDB Rich Metadata (Developer / Genres / Summary) ────────────────
        if let Some(meta) = igdb_meta {
            if !meta.companies.is_empty() {
                let comp_str = format!("Developer: {}", meta.companies.join(", "));
                if let Some((_, th)) = fonts.draw_text(
                    canvas,
                    tc,
                    meta_pt,
                    false,
                    &comp_str,
                    Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, text_alpha),
                    rx + 10,
                    y,
                    false,
                ) {
                    y += th as i32 + 4;
                }
            }
            if !meta.genres.is_empty() {
                let genres_str = format!("Genres: {}", meta.genres.join(", "));
                if let Some((_, th)) = fonts.draw_text(
                    canvas,
                    tc,
                    meta_pt,
                    false,
                    &genres_str,
                    Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, text_alpha),
                    rx + 10,
                    y,
                    false,
                ) {
                    y += th as i32 + 4;
                }
            }
            let summary = meta.summary.trim();
            if !summary.is_empty() {
                y += 2;
                let wrapped = {
                    let hf = fonts.get_font(hint_pt, false);
                    wrap_text(summary, hf, (w - 20).max(20) as u32)
                };
                for line in wrapped.iter().take(3) {
                    if let Some((_, th)) = fonts.draw_text(
                        canvas,
                        tc,
                        hint_pt,
                        false,
                        line,
                        Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, text_alpha),
                        rx + 10,
                        y,
                        false,
                    ) {
                        y += th as i32 + 2;
                    }
                }
            }
        }

        // ── Web Links (Static position near bottom) ──────────────────────────
        self.link_rects.clear();
        let mut links: Vec<(&str, String)> = Vec::new();
        let slug_low = game.slug.to_lowercase();
        let title_low = game.name.to_lowercase();

        if title_low.contains("world of warcraft") {
            links.push(("wowhead", "https://www.wowhead.com".to_string()));
            links.push(("mmo-champion", "https://www.mmo-champion.com".to_string()));
            links.push(("raider.io", "https://raider.io".to_string()));
            links.push(("archon", "https://www.archon.gg".to_string()));
            links.push(("raidbots", "https://www.raidbots.com".to_string()));
        } else if title_low.contains("diablo") {
            if title_low.contains("ii") || title_low.contains('2') {
                links.push(("maxroll diablo ii", "https://maxroll.gg/d2".to_string()));
            } else if title_low.contains("iii") || title_low.contains('3') {
                links.push(("maxroll diablo iii", "https://maxroll.gg/d3".to_string()));
            } else if title_low.contains("iv") || title_low.contains('4') {
                links.push(("maxroll diablo iv", "https://maxroll.gg/d4".to_string()));
            } else {
                links.push(("maxroll", "https://maxroll.gg".to_string()));
            }
        } else if title_low.contains("path of exile 2") || slug_low.contains("poe2") {
            links.push((
                "path of exile 2",
                "https://www.pathofexile.com/poe2".to_string(),
            ));
            links.push(("maxroll poe 2", "https://maxroll.gg/poe2".to_string()));
        } else if title_low.contains("path of exile") || slug_low.contains("poe") {
            links.push(("path of exile", "https://www.pathofexile.com".to_string()));
            links.push(("maxroll poe", "https://maxroll.gg/poe".to_string()));
        } else if title_low.contains("satisfactory") {
            links.push((
                "satisfactory calculator",
                "https://www.satisfactory-calculator.com".to_string(),
            ));
            links.push((
                "satisfactory tools",
                "https://www.satisfactorytools.com".to_string(),
            ));
        } else if title_low.contains("minecraft") {
            links.push(("minecraft wiki", "https://minecraft.wiki".to_string()));
        }

        if game.platform == Platform::Steam && game.steam_appid.is_some() && !game.is_shortcut {
            let appid = game.steam_appid.as_deref().unwrap_or_default();
            links.push((
                "steam store",
                format!("https://store.steampowered.com/app/{}", appid),
            ));
            links.push((
                "steam community",
                format!("https://steamcommunity.com/app/{}", appid),
            ));
        } else if links.is_empty() {
            if game.platform == Platform::Lutris {
                let lslug = game.lutris_slug.as_deref().unwrap_or(&game.slug);
                links.push(("lutris page", format!("https://lutris.net/games/{}", lslug)));
                let pcgw = if let Some(ref appid) = game.steam_appid {
                    format!("https://www.pcgamingwiki.com/api/appid.php?appid={}", appid)
                } else {
                    format!(
                        "https://www.pcgamingwiki.com/w/index.php?search={}",
                        game.name
                    )
                };
                links.push(("pcgamingwiki", pcgw));
            } else {
                links.push((
                    "pcgamingwiki",
                    format!(
                        "https://www.pcgamingwiki.com/w/index.php?search={}",
                        game.name
                    ),
                ));
            }
        }

        if !links.is_empty() {
            let f1_height = 30;
            let spacing = 24;
            let total_link_h = (links.len() as i32) * spacing;
            let link_start_y = h - f1_height - total_link_h - 15;
            let (mx, my) = mouse_pos;

            for (i, (name, url)) in links.into_iter().enumerate() {
                let lnk_y = ry + link_start_y + (i as i32) * spacing;
                let (tw, th) = fonts.size_of(meta_pt, false, name).unwrap_or((100, 18));
                let click_rect = Rect::new(rx + 10, lnk_y, tw, th);

                let color = if self.clicked_url.as_deref() == Some(url.as_str())
                    && self.click_timer > 0.0
                {
                    Color::RGBA(255, 0, 255, text_alpha)
                } else if click_rect.contains_point((mx, my)) {
                    Color::RGBA(0, 255, 255, text_alpha)
                } else {
                    Color::RGBA(102, 0, 255, text_alpha)
                };

                let _ = fonts.draw_text(
                    canvas,
                    tc,
                    meta_pt,
                    false,
                    name,
                    color,
                    rx + 10,
                    lnk_y,
                    false,
                );
                self.link_rects.push((click_rect, url));
            }
        }

        // ── F1 hint ──────────────────────────────────────────────────────────
        let f1_h = fonts.height(hint_pt, false);
        let _ = fonts.draw_text(
            canvas,
            tc,
            hint_pt,
            false,
            "F1  Keybinds",
            Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, text_alpha),
            rx + 10,
            ry + h - f1_h - 10,
            false,
        );
    }

    pub fn handle_click(&mut self, mx: i32, my: i32) -> bool {
        for (rect, url) in &self.link_rects {
            if rect.contains_point((mx, my)) {
                self.clicked_url = Some(url.clone());
                self.click_timer = 0.15;
                let u = url.clone();
                let _ = Command::new("xdg-open").arg(u).spawn();
                return true;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Details Overlay
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsAction {
    Launch,
    ToggleHide,
    NextCoverArt,
    SelectCustomArt,
    RemoveCustomArt,
}

pub struct DetailsOverlay {
    screen_w: u32,
    screen_h: u32,
    pub game: Option<Game>,
    open: bool,
    pub alpha_tw: Tween,
    alpha: f64,
    action_idx: usize,
}

impl DetailsOverlay {
    pub fn new(screen_size: (u32, u32)) -> Self {
        Self {
            screen_w: screen_size.0,
            screen_h: screen_size.1,
            game: None,
            open: false,
            alpha_tw: Tween::new(0.0, 255.0, 0.18),
            alpha: 0.0,
            action_idx: 0,
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.screen_w = w;
        self.screen_h = h;
    }

    pub fn open(&mut self, game: Game) {
        self.game = Some(game);
        self.open = true;
        self.action_idx = 0;
        self.alpha_tw.reset(0.0, 255.0, Some(0.18));
    }

    pub fn close(&mut self) {
        self.open = false;
        self.game = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn build_actions(&self) -> Vec<(&'static str, DetailsAction)> {
        let Some(ref g) = self.game else {
            return Vec::new();
        };
        let is_hid = hidden::is_hidden(&g.slug);
        let has_custom = art::has_custom_art(&g.slug);
        let mut actions = vec![
            ("Launch game", DetailsAction::Launch),
            (
                if is_hid { "Unhide game" } else { "Hide game" },
                DetailsAction::ToggleHide,
            ),
        ];
        if !has_custom {
            actions.push(("Next Cover Art", DetailsAction::NextCoverArt));
        }
        actions.push((
            if has_custom {
                "Change Custom Art"
            } else {
                "Add Custom Art"
            },
            DetailsAction::SelectCustomArt,
        ));
        if has_custom {
            actions.push(("Remove Custom Art", DetailsAction::RemoveCustomArt));
        }
        actions
    }

    pub fn handle_up(&mut self) {
        let n = self.build_actions().len();
        if n > 0 {
            self.action_idx = (self.action_idx + n - 1) % n;
        }
    }

    pub fn handle_down(&mut self) {
        let n = self.build_actions().len();
        if n > 0 {
            self.action_idx = (self.action_idx + 1) % n;
        }
    }

    /// Execute selected action. Returns `(should_close, Option<Game to launch>)`.
    pub fn handle_confirm(&mut self) -> (bool, Option<Game>) {
        let Some(game) = self.game.clone() else {
            return (true, None);
        };
        let actions = self.build_actions();
        let Some(&(_, act)) = actions.get(self.action_idx) else {
            return (true, None);
        };

        match act {
            DetailsAction::Launch => (true, Some(game)),
            DetailsAction::ToggleHide => {
                hidden::toggle(&game.slug);
                (true, None)
            }
            DetailsAction::NextCoverArt => {
                let g_clone = game.clone();
                thread::spawn(move || {
                    art::swap_to_next_art(&g_clone);
                });
                (false, None)
            }
            DetailsAction::SelectCustomArt => {
                let slug = game.slug.clone();
                thread::spawn(move || {
                    if let Ok(res) = Command::new("zenity")
                        .args([
                            "--file-selection",
                            "--title=Select Custom Art",
                            "--file-filter=Images (png, jpg, jpeg, webp) | *.png *.jpg *.jpeg *.webp",
                        ])
                        .output()
                    {
                        if res.status.success() {
                            let stdout = String::from_utf8_lossy(&res.stdout);
                            let src = PathBuf::from(stdout.trim());
                            if src.exists() {
                                let dest_dir = config::custom_art_dir();
                                let _ = fs::create_dir_all(&dest_dir);
                                for ext in [".png", ".jpg", ".jpeg", ".webp"] {
                                    let old_p = dest_dir.join(format!("{}{}", slug, ext));
                                    if old_p.exists() {
                                        let _ = fs::remove_file(&old_p);
                                    }
                                }
                                let ext = src
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| format!(".{}", e.to_lowercase()))
                                    .unwrap_or_else(|| ".jpg".to_string());
                                let dest = dest_dir.join(format!("{}{}", slug, ext));
                                if fs::copy(&src, &dest).is_ok() {
                                    art::invalidate(&slug);
                                }
                            }
                        }
                    }
                });
                (false, None)
            }
            DetailsAction::RemoveCustomArt => {
                if art::remove_custom_art(&game.slug) {
                    let n = self.build_actions().len();
                    if n > 0 {
                        self.action_idx = self.action_idx.min(n - 1);
                    }
                }
                (false, None)
            }
        }
    }

    pub fn update(&mut self, dt: f64) {
        if !self.open || self.game.is_none() {
            return;
        }
        self.alpha_tw.update(dt);
        self.alpha = self.alpha_tw.value;
        let n = self.build_actions().len();
        if n > 0 && self.action_idx >= n {
            self.action_idx = n - 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw<'ttf>(
        &self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        fonts: &mut FontCache<'ttf>,
        art_cache: &mut ArtCache,
        icons: &mut IconCache,
        shift_x: i32,
        shift_y: i32,
    ) {
        if !self.open {
            return;
        }
        let Some(ref game) = self.game else {
            return;
        };

        let sw = self.screen_w as i32;
        let sh = self.screen_h as i32;
        let alpha = self.alpha.clamp(0.0, 255.0) as u8;

        let title_pt: u16 = 32;
        let meta_pt: u16 = 18;
        let action_pt: u16 = 20;
        let hint_pt: u16 = 15;

        canvas.set_blend_mode(BlendMode::Blend);

        // Dark veil
        canvas.set_draw_color(Color::RGBA(0, 0, 0, alpha.min(200)));
        let _ = canvas.fill_rect(Rect::new(0, 0, self.screen_w, self.screen_h));

        // Card panel
        let pw = (sw - 80).clamp(320, 720);
        let ph = (sh - 80).clamp(280, 600);
        let progress = (alpha as f64) / 255.0;
        let slide_offset = ((1.0 - progress) * 60.0) as i32;

        let px = (sw - pw) / 2 + slide_offset + shift_x;
        let py = (sh - ph) / 2 + shift_y;
        let panel_rect = Rect::new(px, py, pw as u32, ph as u32);

        canvas.set_draw_color(Color::RGBA(
            config::BG_PANEL.0,
            config::BG_PANEL.1,
            config::BG_PANEL.2,
            alpha.min(245),
        ));
        let _ = canvas.fill_rect(panel_rect);

        // Left column art
        let art_col_w = pw / 3;
        let art_w = (art_col_w - 20).max(20);
        let art_h = art_w * 900 / 600;
        let art_y = py + (ph - art_h) / 2;
        let art_rect = Rect::new(px + 10, art_y, art_w as u32, art_h as u32);

        if let Some(tex) = art_cache.get_or_load_texture(tc, game) {
            tex.set_alpha_mod(alpha);
            let _ = canvas.copy(tex, None, Some(art_rect));
            tex.set_alpha_mod(255);
        } else {
            art::draw_placeholder(
                canvas, tc, fonts, title_pt, meta_pt, game, art_rect, alpha,
            );
        }
        canvas.set_draw_color(Color::RGBA(
            config::BORDER_BG.0,
            config::BORDER_BG.1,
            config::BORDER_BG.2,
            alpha,
        ));
        let _ = canvas.draw_rect(art_rect);

        // Right column info + actions
        let rx = px + art_col_w + 10;
        let rw = (pw - (art_col_w + 10) - 16).max(60);
        let mut ry = py + 20;

        // Platform badge
        let plat = game.display_platform_label().to_uppercase();
        let bc = colour_for_platform(game.platform);
        if let Ok((tw, th)) = fonts.size_of(meta_pt, false, &plat) {
            let bw = tw + 12;
            let bh = th + 6;
            canvas.set_draw_color(Color::RGBA(bc.0, bc.1, bc.2, alpha));
            let _ = canvas.fill_rect(Rect::new(rx, ry, bw, bh));
            let _ = fonts.draw_text(
                canvas,
                tc,
                meta_pt,
                false,
                &plat,
                Color::RGBA(
                    config::BG_DEEP.0,
                    config::BG_DEEP.1,
                    config::BG_DEEP.2,
                    alpha,
                ),
                rx + 6,
                ry + 3,
                false,
            );
            ry += bh as i32 + 10;
        }

        if game.launch_via_lutris {
            if let Some((_, th)) = fonts.draw_text(
                canvas,
                tc,
                meta_pt,
                false,
                "via Lutris",
                Color::RGBA(
                    config::BADGE_LUTRIS.0,
                    config::BADGE_LUTRIS.1,
                    config::BADGE_LUTRIS.2,
                    alpha,
                ),
                rx,
                ry,
                false,
            ) {
                ry += th as i32 + 4;
            }
        }

        // Title
        let title_lines = {
            let tf = fonts.get_font(title_pt, true);
            wrap_text(&game.name, tf, rw as u32)
        };
        for line in title_lines.iter().take(3) {
            if let Some((_, th)) = fonts.draw_text(
                canvas,
                tc,
                title_pt,
                true,
                line,
                Color::RGBA(config::WHITE.0, config::WHITE.1, config::WHITE.2, alpha),
                rx,
                ry,
                false,
            ) {
                ry += th as i32 + 2;
            }
        }
        ry += 8;

        if let Some(yr) = game.year {
            if let Some((_, th)) = fonts.draw_text(
                canvas,
                tc,
                meta_pt,
                false,
                &yr.to_string(),
                Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, alpha),
                rx,
                ry,
                false,
            ) {
                ry += th as i32 + 4;
            }
        }

        if !matches!(game.platform, Platform::Lutris | Platform::Battlenet) {
            let pt = game.playtime_str();
            if !pt.is_empty() {
                if let Some((_, th)) = fonts.draw_text(
                    canvas,
                    tc,
                    meta_pt,
                    false,
                    &format!("Playtime: {}", pt),
                    Color::RGBA(config::CYAN.0, config::CYAN.1, config::CYAN.2, alpha),
                    rx,
                    ry,
                    false,
                ) {
                    ry += th as i32 + 4;
                }
            }
        }

        if hidden::is_hidden(&game.slug) {
            if let Some((_, th)) = fonts.draw_text(
                canvas,
                tc,
                meta_pt,
                false,
                "HIDDEN",
                Color::RGBA(
                    config::MAGENTA.0,
                    config::MAGENTA.1,
                    config::MAGENTA.2,
                    alpha,
                ),
                rx,
                ry,
                false,
            ) {
                ry += th as i32 + 4;
            }
        }

        ry += 16;

        // Action list
        let actions = self.build_actions();
        let action_icon_map = ["cross", "square", "triangle", "options"];
        let row_h = fonts.height(action_pt, true) + 10;

        for (i, (label, _)) in actions.iter().enumerate() {
            let selected = i == self.action_idx;
            let row_rect = Rect::new(rx - 4, ry - 4, (rw + 8) as u32, (row_h + 8) as u32);
            if selected {
                canvas.set_draw_color(Color::RGBA(
                    config::VIOLET.0,
                    config::VIOLET.1,
                    config::VIOLET.2,
                    ((60.0 / 255.0) * (alpha as f32)) as u8,
                ));
                let _ = canvas.fill_rect(row_rect);
                canvas.set_draw_color(Color::RGBA(
                    config::BORDER_BG.0,
                    config::BORDER_BG.1,
                    config::BORDER_BG.2,
                    alpha,
                ));
                let _ = canvas.draw_rect(row_rect);
            }

            let icon_name = action_icon_map.get(i).copied().unwrap_or("cross");
            let c = if selected {
                Color::RGBA(config::WHITE.0, config::WHITE.1, config::WHITE.2, alpha)
            } else {
                Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, alpha)
            };
            icons.draw_hint(
                canvas,
                tc,
                fonts,
                rx + 2,
                ry + 2,
                icon_name,
                label,
                action_pt,
                true,
                22,
                c,
                8,
            );
            ry += row_h + 8;
        }

        // Bottom hint bar
        let hint_y = py + ph - fonts.height(hint_pt, false) - 14;
        canvas.set_draw_color(Color::RGBA(
            config::BORDER_BG.0,
            config::BORDER_BG.1,
            config::BORDER_BG.2,
            alpha,
        ));
        let _ = canvas.draw_line((rx, hint_y - 8), (px + pw - 10, hint_y - 8));

        let mut cx = rx;
        for (icon_name, label) in [("dpad", "Navigate"), ("circle", "Close")] {
            if let Some(tex) = icons.get(tc, icon_name) {
                tex.set_alpha_mod(alpha);
                let _ = canvas.copy(tex, None, Some(Rect::new(cx, hint_y + 2, 16, 16)));
                tex.set_alpha_mod(255);
                cx += 20;
            }
            if let Some((tw, _)) = fonts.draw_text(
                canvas,
                tc,
                hint_pt,
                false,
                label,
                Color::RGBA(config::GREY.0, config::GREY.1, config::GREY.2, alpha),
                cx,
                hint_y,
                false,
            ) {
                cx += tw as i32 + 20;
            }
        }

        // Panel border
        canvas.set_draw_color(Color::RGBA(
            config::BORDER_BG.0,
            config::BORDER_BG.1,
            config::BORDER_BG.2,
            alpha,
        ));
        let _ = canvas.draw_rect(panel_rect);
    }
}

// ---------------------------------------------------------------------------
// Keybinds Overlay
// ---------------------------------------------------------------------------

const KEYBINDS: &[(&str, &[(&str, &str)])] = &[
    (
        "Navigation",
        &[
            ("Arrow Keys / WASD", "Move selection"),
            ("Mouse Click", "Select game"),
            ("Mouse Wheel", "Scroll grid"),
            ("Page Up / Page Down", "Scroll by page"),
        ],
    ),
    (
        "Actions",
        &[
            ("Enter / Space", "Launch game"),
            ("Double-Click", "Launch game"),
            ("I", "Details / Actions"),
            ("H", "Hide / Unhide game"),
            ("Tab", "Toggle show hidden"),
            ("F", "Toggle FPS Limit"),
            ("N", "Toggle Animations"),
            ("F5", "Refresh library"),
        ],
    ),
    (
        "Controller",
        &[
            ("✕  Cross", "Launch"),
            ("△  Triangle", "Details / Actions"),
            ("□  Square", "Details / Actions"),
            ("○  Circle", "Back / Close overlay"),
            ("D-Pad Up", "Hide / Unhide game"),
            ("D-Pad / Left Stick", "Navigate"),
            ("L1 / R1", "Page scroll"),
            ("Create", "Toggle show hidden"),
            ("Options", "Refresh library"),
        ],
    ),
];

pub struct KeybindsOverlay {
    sw: u32,
    sh: u32,
    open: bool,
    pub pixel_shift_enabled: bool,
}

impl KeybindsOverlay {
    pub fn new(screen_size: (u32, u32)) -> Self {
        Self {
            sw: screen_size.0,
            sh: screen_size.1,
            open: false,
            pixel_shift_enabled: config::get_bool("ui", "pixel_shift"),
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.sw = w;
        self.sh = h;
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle_pixel_shift(&mut self) {
        self.pixel_shift_enabled = !self.pixel_shift_enabled;
        config::set_val(
            "ui",
            "pixel_shift",
            ConfigVal::Bool(self.pixel_shift_enabled),
        );
    }

    pub fn toggle_fps(&mut self, canvas: &mut Canvas<Window>, target_fps: &mut u32) {
        let vsync_enabled = config::get_bool("general", "vsync");
        let fps_limit = config::get_int("general", "fps");

        let (next_vsync, next_fps) = if vsync_enabled {
            (false, 30)
        } else if fps_limit == 30 {
            (false, 60)
        } else if fps_limit == 60 {
            (false, 144)
        } else {
            (true, 144)
        };

        config::set_val("general", "vsync", ConfigVal::Bool(next_vsync));
        config::set_val("general", "fps", ConfigVal::Int(next_fps));
        *target_fps = next_fps as u32;

        unsafe {
            sdl2::sys::SDL_RenderSetVSync(canvas.raw(), if next_vsync { 1 } else { 0 });
        }
    }

    pub fn toggle_animations(&mut self, grid: &mut Grid) {
        let current_anim = config::get_bool("ui", "animations");
        config::set_val("ui", "animations", ConfigVal::Bool(!current_anim));
        if current_anim {
            grid.scroll_y = grid.target_y;
            grid.dirty = true;
        }
    }

    pub fn draw<'ttf>(
        &self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        fonts: &mut FontCache<'ttf>,
        icons: &mut IconCache,
        shift_x: i32,
        shift_y: i32,
    ) {
        if !self.open {
            return;
        }

        let sw = self.sw as i32;
        let sh = self.sh as i32;

        let heading_pt: u16 = 20;
        let key_pt: u16 = 14;
        let desc_pt: u16 = 14;
        let title_pt: u16 = 28;
        let hint_pt: u16 = 12;

        canvas.set_blend_mode(BlendMode::Blend);

        // Dark veil
        canvas.set_draw_color(Color::RGBA(0, 0, 0, 210));
        let _ = canvas.fill_rect(Rect::new(0, 0, self.sw, self.sh));

        // Panel
        let pw = (sw - 60).clamp(320, 680);
        let ph = (sh - 60).clamp(300, 560);
        let px = (sw - pw) / 2 + shift_x;
        let py = (sh - ph) / 2 + shift_y;
        let panel_rect = Rect::new(px, py, pw as u32, ph as u32);

        canvas.set_draw_color(Color::RGBA(
            config::BG_PANEL.0,
            config::BG_PANEL.1,
            config::BG_PANEL.2,
            250,
        ));
        let _ = canvas.fill_rect(panel_rect);

        // Title
        let _ = fonts.draw_text(
            canvas,
            tc,
            title_pt,
            true,
            "Keybinds",
            Color::RGB(config::WHITE.0, config::WHITE.1, config::WHITE.2),
            px + pw / 2,
            py + 16,
            true,
        );

        let col_w = (pw - 40) / (KEYBINDS.len() as i32);
        let mut cx = px + 20;
        let top_y = py + 60;
        let key_font_h = fonts.height(key_pt, true);

        for (section_name, bindings) in KEYBINDS {
            let mut y = top_y;
            if let Some((_, hh)) = fonts.draw_text(
                canvas,
                tc,
                heading_pt,
                true,
                section_name,
                Color::RGB(config::VIOLET.0, config::VIOLET.1, config::VIOLET.2),
                cx,
                y,
                false,
            ) {
                y += hh as i32 + 8;
            }

            canvas.set_draw_color(Color::RGB(
                config::BORDER_BG.0,
                config::BORDER_BG.1,
                config::BORDER_BG.2,
            ));
            let _ = canvas.draw_line((cx, y), (cx + col_w - 20, y));
            y += 6;

            for (key_label, desc_label) in *bindings {
                let mut drew_icon = false;
                let mut kh = key_font_h;

                if *section_name == "Controller" {
                    let low = key_label.to_lowercase();
                    let mapped = if low.contains("cross") {
                        Some("cross")
                    } else if low.contains("circle") {
                        Some("circle")
                    } else if low.contains("triangle") {
                        Some("triangle")
                    } else if low.contains("square") {
                        Some("square")
                    } else if low.contains("d-pad up") {
                        Some("dpad_up")
                    } else if low.contains("d-pad") {
                        Some("dpad")
                    } else if low.contains("l1") {
                        Some("l1")
                    } else if low.contains("r1") {
                        Some("r1")
                    } else if low.contains("create") {
                        Some("create")
                    } else if low.contains("options") {
                        Some("options")
                    } else {
                        None
                    };

                    if let Some(icon_name) = mapped {
                        if let Some(tex) = icons.get(tc, icon_name) {
                            let iy = y + (key_font_h - 18) / 2;
                            tex.set_alpha_mod(255);
                            let _ = canvas.copy(tex, None, Some(Rect::new(cx, iy, 18, 18)));
                            let short_label = key_label
                                .split_whitespace()
                                .skip(1)
                                .collect::<Vec<_>>()
                                .join(" ");
                            let text_to_draw = if short_label.is_empty()
                                || key_label.starts_with("D-Pad")
                                || key_label.starts_with("L1")
                                || key_label.starts_with("Create")
                                || key_label.starts_with("Options")
                            {
                                *key_label
                            } else {
                                &short_label
                            };
                            if let Some((_, h)) = fonts.draw_text(
                                canvas,
                                tc,
                                key_pt,
                                true,
                                text_to_draw,
                                Color::RGB(config::CYAN.0, config::CYAN.1, config::CYAN.2),
                                cx + 24,
                                y,
                                false,
                            ) {
                                kh = h as i32;
                            }
                            drew_icon = true;
                        }
                    }
                }

                if !drew_icon {
                    if let Some((_, h)) = fonts.draw_text(
                        canvas,
                        tc,
                        key_pt,
                        true,
                        key_label,
                        Color::RGB(config::CYAN.0, config::CYAN.1, config::CYAN.2),
                        cx,
                        y,
                        false,
                    ) {
                        kh = h as i32;
                    }
                }

                y += kh + 1;
                if let Some((_, dh)) = fonts.draw_text(
                    canvas,
                    tc,
                    desc_pt,
                    false,
                    desc_label,
                    Color::RGB(config::GREY.0, config::GREY.1, config::GREY.2),
                    cx + 8,
                    y,
                    false,
                ) {
                    y += dh as i32 + 8;
                }
            }

            cx += col_w;
        }

        // Bottom hint
        let hint_h = fonts.height(hint_pt, false);
        let _ = fonts.draw_text(
            canvas,
            tc,
            hint_pt,
            false,
            "Press F1 or Esc to close",
            Color::RGB(config::GREY.0, config::GREY.1, config::GREY.2),
            px + pw / 2,
            py + ph - hint_h - 12,
            true,
        );

        // Settings strip
        let strip_h = key_font_h + 14;
        let strip_y = py + ph - strip_h - hint_h - 22;
        canvas.set_draw_color(Color::RGB(
            config::BORDER_BG.0,
            config::BORDER_BG.1,
            config::BORDER_BG.2,
        ));
        let _ = canvas.draw_line((px + 20, strip_y - 8), (px + pw - 20, strip_y - 8));

        let mut pill_x = px + 20;
        if let Some((sw_lbl, _)) = fonts.draw_text(
            canvas,
            tc,
            heading_pt,
            true,
            "Settings",
            Color::RGB(config::VIOLET.0, config::VIOLET.1, config::VIOLET.2),
            pill_x,
            strip_y,
            false,
        ) {
            pill_x += sw_lbl as i32 + 12;
        }

        // 1. Pixel shift pill
        let ps_lbl = format!(
            "Pixel Shift: {}",
            if self.pixel_shift_enabled { "ON" } else { "OFF" }
        );
        // 2. Frame Limit pill
        let vsync_enabled = config::get_bool("general", "vsync");
        let fps_limit = config::get_int("general", "fps");
        let fps_lbl = if vsync_enabled {
            "Frame Limit: VSYNC".to_string()
        } else {
            format!("Frame Limit: {} FPS", fps_limit)
        };
        // 3. Animations pill
        let anim_enabled = config::get_bool("ui", "animations");
        let anim_lbl = format!("Animations: {}", if anim_enabled { "ON" } else { "OFF" });

        for (key_txt, lbl_txt, active) in [
            ("[P]", ps_lbl.as_str(), self.pixel_shift_enabled),
            ("[F]", fps_lbl.as_str(), true),
            ("[N]", anim_lbl.as_str(), anim_enabled),
        ] {
            let kw = fonts
                .size_of(key_pt, true, key_txt)
                .map(|(w, _)| w as i32)
                .unwrap_or(24);
            let lw = fonts
                .size_of(key_pt, true, lbl_txt)
                .map(|(w, _)| w as i32)
                .unwrap_or(80);
            let pill_w = kw + lw + 16;
            let pill_rect = Rect::new(pill_x - 6, strip_y - 3, pill_w as u32, strip_h as u32);
            if active {
                canvas.set_draw_color(Color::RGBA(
                    config::VIOLET.0,
                    config::VIOLET.1,
                    config::VIOLET.2,
                    30,
                ));
            } else {
                canvas.set_draw_color(Color::RGBA(30, 30, 30, 80));
            }
            let _ = canvas.fill_rect(pill_rect);

            let border_c = if active { config::CYAN } else { config::GREY };
            canvas.set_draw_color(Color::RGB(border_c.0, border_c.1, border_c.2));
            let _ = canvas.draw_rect(pill_rect);

            let _ = fonts.draw_text(
                canvas,
                tc,
                key_pt,
                true,
                key_txt,
                Color::RGB(config::CYAN.0, config::CYAN.1, config::CYAN.2),
                pill_x,
                strip_y + 4,
                false,
            );
            let _ = fonts.draw_text(
                canvas,
                tc,
                key_pt,
                true,
                lbl_txt,
                Color::RGB(border_c.0, border_c.1, border_c.2),
                pill_x + kw + 8,
                strip_y + 4,
                false,
            );

            pill_x += pill_w + 16;
        }

        // Border
        canvas.set_draw_color(Color::RGB(
            config::BORDER_BG.0,
            config::BORDER_BG.1,
            config::BORDER_BG.2,
        ));
        let _ = canvas.draw_rect(panel_rect);
    }
}
