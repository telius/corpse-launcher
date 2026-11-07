mod art;
mod audio;
mod config;
mod game;
mod hidden;
mod igdb;
mod input;
mod launcher;
mod library;
mod lutris;
mod sgdb;
mod steam;
mod ui;
mod vdf;

use std::env;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sdl2::image::InitFlag;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas};
use sdl2::ttf::Sdl2TtfContext;
use sdl2::video::Window;

use crate::art::ArtCache;
use crate::audio::SoundSystem;
use crate::game::Game;
use crate::input::{Action, InputHandler};
use crate::ui::{DetailsOverlay, FontCache, Grid, IconCache, KeybindsOverlay, Sidebar};

static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn sig_handler(_signum: libc::c_int) {
    STOP_REQUESTED.store(true, Ordering::SeqCst);
}

fn setup_env_and_signals() {
    unsafe {
        libc::signal(libc::SIGTERM, sig_handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, sig_handler as libc::sighandler_t);
    }

    if env::var_os("SDL_VIDEODRIVER").is_none() {
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            env::set_var("SDL_VIDEODRIVER", "wayland");
        } else if env::var_os("DISPLAY").is_some() {
            env::set_var("SDL_VIDEODRIVER", "x11");
        }
    }

    if env::var_os("SDL_RENDER_DRIVER").is_none() {
        env::set_var("SDL_RENDER_DRIVER", "opengl");
    }
    if env::var_os("SDL_JOYSTICK_HIDAPI_PS5").is_none() {
        env::set_var("SDL_JOYSTICK_HIDAPI_PS5", "1");
    }
    if env::var_os("SDL_JOYSTICK_HIDAPI_PS5_RUMBLE").is_none() {
        env::set_var("SDL_JOYSTICK_HIDAPI_PS5_RUMBLE", "1");
    }
    if env::var_os("SDL_VIDEO_ALLOW_SCREENSAVER").is_none() {
        env::set_var("SDL_VIDEO_ALLOW_SCREENSAVER", "0");
    }
    if env::var_os("SDL_VIDEO_WAYLAND_WMCLASS").is_none() {
        env::set_var("SDL_VIDEO_WAYLAND_WMCLASS", "corpse-launcher");
    }
    if env::var_os("SDL_APP_ID").is_none() {
        env::set_var("SDL_APP_ID", "corpse-launcher");
    }
    if env::var_os("SDL_RENDER_SCALE_QUALITY").is_none() {
        env::set_var("SDL_RENDER_SCALE_QUALITY", "linear");
    }
}

fn sidebar_w(win_w: u32) -> u32 {
    ((win_w as f64 * 0.22) as u32).clamp(220, 380)
}

fn dry_run() {
    config::init();
    let games = library::load(false);
    let line = "─".repeat(60);
    println!("\n{}", line);
    println!("  corpse-launcher — dry run  ({} games)", games.len());
    println!("{}", line);
    for g in &games {
        let via = if g.launch_via_lutris { " [Lutris]" } else { "" };
        let has_art = if g.art_path.is_some() { " [art]" } else { "" };
        let hid = if hidden::is_hidden(&g.slug) {
            " [hidden]"
        } else {
            ""
        };
        println!(
            "  {:<40} {:<10}{}{}{}",
            g.name,
            g.platform.as_str(),
            via,
            has_art,
            hid
        );
    }
    println!("{}\n", line);
}

fn draw_loading<'ttf>(
    canvas: &mut Canvas<Window>,
    tc: &sdl2::render::TextureCreator<sdl2::video::WindowContext>,
    fonts: &mut FontCache<'ttf>,
    win_size: (u32, u32),
    msg: &str,
) {
    let (w, h) = win_size;
    canvas.set_blend_mode(BlendMode::Blend);
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 160));
    let _ = canvas.fill_rect(Rect::new(0, 0, w, h));

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs_f64();
    let dots = ".".repeat(((secs * 2.0) as usize) % 4);
    let full = format!("{}{}", msg, dots);
    let fh = fonts.height(24, true);
    let _ = fonts.draw_text(
        canvas,
        tc,
        24,
        true,
        &full,
        Color::RGB(config::CYAN.0, config::CYAN.1, config::CYAN.2),
        (w as i32) / 2,
        (h as i32) / 2 - fh / 2,
        true,
    );
}

fn draw_status<'ttf>(
    canvas: &mut Canvas<Window>,
    tc: &sdl2::render::TextureCreator<sdl2::video::WindowContext>,
    fonts: &mut FontCache<'ttf>,
    win_size: (u32, u32),
    msg: &str,
) {
    let (_, sh) = win_size;
    let Ok((tw, th)) = fonts.size_of(14, false, msg) else {
        return;
    };
    let pad = 8;
    let w = tw + (pad as u32) * 2;
    let h = th + (pad as u32);
    let x = 12;
    let y = (sh as i32) - (h as i32) - 12;
    let rect = Rect::new(x, y, w, h);

    canvas.set_blend_mode(BlendMode::Blend);
    canvas.set_draw_color(Color::RGBA(
        config::BG_PANEL.0,
        config::BG_PANEL.1,
        config::BG_PANEL.2,
        200,
    ));
    let _ = canvas.fill_rect(rect);

    canvas.set_draw_color(Color::RGB(
        config::BORDER_BG.0,
        config::BORDER_BG.1,
        config::BORDER_BG.2,
    ));
    let _ = canvas.draw_rect(rect);

    let _ = fonts.draw_text(
        canvas,
        tc,
        14,
        false,
        msg,
        Color::RGB(config::CYAN.0, config::CYAN.1, config::CYAN.2),
        x + pad,
        y + pad / 2,
        false,
    );
}

struct App<'ttf> {
    fonts: FontCache<'ttf>,
    canvas: Canvas<Window>,
    tc: sdl2::render::TextureCreator<sdl2::video::WindowContext>,
    event_pump: sdl2::EventPump,
    input: InputHandler,
    sounds: SoundSystem,
    art_cache: ArtCache,
    icons: IconCache,

    target_fps: u32,
    idle_fps: u32,
    running: bool,

    all_games: Vec<Game>,
    pending_games: Arc<Mutex<Option<Vec<Game>>>>,
    loading: bool,
    status: String,
    status_clear_at: Option<Instant>,
    show_hidden: bool,
    needs_draw: bool,
    win_size: (u32, u32),

    grid: Grid,
    sidebar: Sidebar,
    details: DetailsOverlay,
    keybinds: KeybindsOverlay,

    idle_time: f64,
    active_mouse_pos: (i32, i32),
    shift_x: i32,
    shift_y: i32,
    dimmed_gc_done: bool,
    start_instant: Instant,
}

impl<'ttf> App<'ttf> {
    fn new(sdl: &sdl2::Sdl, ttf: &'ttf Sdl2TtfContext) -> Result<Self, String> {
        config::init();

        let video = sdl.video()?;
        let audio_sub = sdl.audio().ok();
        let joy_sub = sdl.joystick().ok();
        let event_pump = sdl.event_pump()?;

        let (w, h) = config::window_size();
        let window = video
            .window("☠  corpse-launcher", w, h)
            .resizable()
            .opengl()
            .build()
            .map_err(|e| e.to_string())?;

        let mut canvas_builder = window.into_canvas().accelerated();
        if config::get_bool("general", "vsync") {
            canvas_builder = canvas_builder.present_vsync();
        }
        let canvas = canvas_builder.build().map_err(|e| e.to_string())?;
        let tc = canvas.texture_creator();

        let win_size = canvas.output_size().unwrap_or((w, h));
        let sw = sidebar_w(win_size.0);

        let grid = Grid::new(
            Vec::new(),
            Rect::new(0, 0, win_size.0.saturating_sub(sw), win_size.1),
        );
        let sidebar = Sidebar::new(Rect::new(
            win_size.0.saturating_sub(sw) as i32,
            0,
            sw,
            win_size.1,
        ));
        let details = DetailsOverlay::new(win_size);
        let keybinds = KeybindsOverlay::new(win_size);
        let input = InputHandler::new(joy_sub);
        let sounds = SoundSystem::new(audio_sub.as_ref());

        let pending_games: Arc<Mutex<Option<Vec<Game>>>> = Arc::new(Mutex::new(None));
        {
            let pg = Arc::clone(&pending_games);
            thread::spawn(move || {
                let loaded = library::load(false);
                if let Ok(mut guard) = pg.lock() {
                    *guard = Some(loaded);
                }
            });
        }

        Ok(Self {
            fonts: FontCache::new(ttf),
            canvas,
            tc,
            event_pump,
            input,
            sounds,
            art_cache: ArtCache::new(),
            icons: IconCache::new(),
            target_fps: config::get_int("general", "fps").max(15) as u32,
            idle_fps: 15,
            running: true,
            all_games: Vec::new(),
            pending_games,
            loading: true,
            status: "Loading...".to_string(),
            status_clear_at: None,
            show_hidden: false,
            needs_draw: true,
            win_size,
            grid,
            sidebar,
            details,
            keybinds,
            idle_time: 0.0,
            active_mouse_pos: (0, 0),
            shift_x: 0,
            shift_y: 0,
            dimmed_gc_done: false,
            start_instant: Instant::now(),
        })
    }

    fn visible_games(&self) -> Vec<Game> {
        if self.show_hidden {
            self.all_games
                .iter()
                .filter(|g| hidden::is_hidden(&g.slug))
                .cloned()
                .collect()
        } else {
            self.all_games
                .iter()
                .filter(|g| !hidden::is_hidden(&g.slug))
                .cloned()
                .collect()
        }
    }

    fn refresh_grid(&mut self) {
        let vis = self.visible_games();
        self.grid.set_games(vis);
        self.sidebar.set_game(self.grid.selected_game().cloned());
        self.needs_draw = true;
    }

    fn layout(&mut self) {
        let (w, h) = self.win_size;
        let sw = sidebar_w(w);
        self.grid
            .resize(Rect::new(0, 0, w.saturating_sub(sw), h));
        self.sidebar
            .resize(Rect::new(w.saturating_sub(sw) as i32, 0, sw, h));
        self.details.resize(w, h);
        self.keybinds.resize(w, h);
        self.needs_draw = true;
    }

    fn do_launch(&self, game: &Game) {
        self.sounds.play_launch();
        launcher::launch(game);
    }

    fn set_timed_status(&mut self, msg: String) {
        self.status = msg;
        self.status_clear_at = Some(Instant::now() + Duration::from_secs(2));
        self.needs_draw = true;
    }

    fn handle_action(&mut self, action: Action) {
        if action == Action::FocusGained {
            self.idle_time = 0.0;
            self.needs_draw = true;
            return;
        }

        if action != Action::MouseHover {
            self.needs_draw = true;
        }

        // Keybinds overlay absorbs everything when open
        if self.keybinds.is_open() {
            match action {
                Action::Help | Action::Back | Action::Quit | Action::MouseRightClick => {
                    self.keybinds.close();
                }
                Action::TogglePixelshift => {
                    self.keybinds.toggle_pixel_shift();
                }
                Action::ToggleFps => {
                    self.keybinds
                        .toggle_fps(&mut self.canvas, &mut self.target_fps);
                }
                Action::ToggleAnimations => {
                    self.keybinds.toggle_animations(&mut self.grid);
                }
                _ => {}
            }
            return;
        }

        // HELP toggles keybinds overlay from anywhere
        if action == Action::Help {
            self.keybinds.toggle();
            return;
        }

        // Details overlay absorbs navigation when open
        if self.details.is_open() {
            match action {
                Action::NavUp => {
                    self.details.handle_up();
                    self.sounds.play_nav();
                }
                Action::NavDown => {
                    self.details.handle_down();
                    self.sounds.play_nav();
                }
                Action::Confirm => {
                    let (should_close, to_launch) = self.details.handle_confirm();
                    if let Some(ref g) = to_launch {
                        self.do_launch(g);
                    } else {
                        self.sounds.play_details_confirm();
                    }
                    if should_close {
                        self.details.close();
                    }
                    self.refresh_grid();
                }
                Action::Back | Action::Details | Action::Quit | Action::MouseRightClick => {
                    self.details.close();
                    self.sounds.play_back();
                }
                _ => {}
            }
            return;
        }

        // Normal grid navigation
        let prev = self.grid.selected;

        match action {
            Action::Quit => {
                self.running = false;
            }
            Action::Back => {
                self.sounds.play_back();
            }
            Action::Confirm => {
                if let Some(g) = self.grid.selected_game().cloned() {
                    self.do_launch(&g);
                }
            }
            Action::Details => {
                if let Some(g) = self.grid.selected_game().cloned() {
                    self.details.open(g);
                }
            }
            Action::Hide => {
                if let Some(g) = self.grid.selected_game().cloned() {
                    let now_hidden = hidden::toggle(&g.slug);
                    self.refresh_grid();
                    let label = format!(
                        "{}: {}",
                        if now_hidden { "Hidden" } else { "Unhidden" },
                        g.name
                    );
                    self.set_timed_status(label);
                }
            }
            Action::ShowHidden => {
                self.show_hidden = !self.show_hidden;
                self.refresh_grid();
                let label = if self.show_hidden {
                    "Showing hidden games".to_string()
                } else {
                    "Hiding hidden games".to_string()
                };
                self.set_timed_status(label);
            }
            Action::NavUp => {
                self.grid.move_sel(0, -1);
                self.sounds.play_nav();
            }
            Action::NavDown => {
                self.grid.move_sel(0, 1);
                self.sounds.play_nav();
            }
            Action::NavLeft => {
                self.grid.move_sel(-1, 0);
                self.sounds.play_nav();
            }
            Action::NavRight => {
                self.grid.move_sel(1, 0);
                self.sounds.play_nav();
            }
            Action::PageLeft => {
                self.grid.page(-1);
                self.sounds.play_nav();
            }
            Action::PageRight => {
                self.grid.page(1);
                self.sounds.play_nav();
            }
            Action::ScrollUp => {
                self.grid.scroll_by(-60.0);
                self.sounds.play_nav();
            }
            Action::ScrollDown => {
                self.grid.scroll_by(60.0);
                self.sounds.play_nav();
            }
            Action::MouseClick => {
                let (mx, my) = self.input.mouse_pos;
                if self.sidebar.handle_click(mx, my) {
                    return;
                }
                if let Some(idx) = self.grid.hover_at(mx, my) {
                    self.grid.selected = idx;
                    self.grid.dirty = true;
                    let g = self.grid.selected_game().cloned();
                    self.sidebar.set_game(g.clone());
                    if let Some(game) = g {
                        self.do_launch(&game);
                    }
                }
            }
            Action::MouseHover => {
                let (mx, my) = self.input.mouse_pos;
                if self.sidebar.hover_at(mx, my) {
                    self.needs_draw = true;
                }
                if let Some(idx) = self.grid.hover_at(mx, my) {
                    if idx != self.grid.selected {
                        self.grid.selected = idx;
                        self.grid.dirty = true;
                        self.sidebar.set_game(self.grid.selected_game().cloned());
                        self.needs_draw = true;
                    }
                }
            }
            Action::MouseRightClick => {
                let (mx, my) = self.input.mouse_pos;
                if let Some(idx) = self.grid.hover_at(mx, my) {
                    self.grid.selected = idx;
                    self.grid.dirty = true;
                    let g = self.grid.selected_game().cloned();
                    self.sidebar.set_game(g.clone());
                    if let Some(game) = g {
                        self.details.open(game);
                    }
                }
            }
            Action::Refresh => {
                self.loading = true;
                self.status = "Refreshing...".to_string();
                self.status_clear_at = None;
                library::invalidate();
                let pg = Arc::clone(&self.pending_games);
                thread::spawn(move || {
                    let loaded = library::load(true);
                    if let Ok(mut guard) = pg.lock() {
                        *guard = Some(loaded);
                    }
                });
            }
            _ => {}
        }

        if self.grid.selected != prev {
            self.sidebar.set_game(self.grid.selected_game().cloned());
        }
    }

    fn run(&mut self) {
        let mut last_frame = Instant::now();

        while self.running && !STOP_REQUESTED.load(Ordering::SeqCst) {
            let now = Instant::now();
            let raw_dt = now.duration_since(last_frame).as_secs_f64();
            last_frame = now;
            let dt = raw_dt.min(0.05);

            // Resize detection
            if let Ok(cur_size) = self.canvas.output_size() {
                if cur_size != self.win_size && cur_size.0 > 0 && cur_size.1 > 0 {
                    self.win_size = cur_size;
                    self.layout();
                }
            }

            // Check pending library load
            let maybe_loaded = self
                .pending_games
                .lock()
                .ok()
                .and_then(|mut g| g.take());
            if let Some(games) = maybe_loaded {
                self.all_games = games;
                self.loading = false;
                self.status.clear();
                self.refresh_grid();
            }

            // Check timed status expiry
            if let Some(clear_at) = self.status_clear_at {
                if Instant::now() >= clear_at {
                    self.status.clear();
                    self.status_clear_at = None;
                    self.needs_draw = true;
                }
            }

            // Check background art updates
            if art::has_pending_updates() {
                self.art_cache.process_invalidations();
                self.needs_draw = true;
            }

            // Input polling
            let actions = self.input.poll(&mut self.event_pump, dt);

            // Inactivity Autodimmer timer update
            let mouse_pos = self.input.mouse_pos;
            let dx = mouse_pos.0 - self.active_mouse_pos.0;
            let dy = mouse_pos.1 - self.active_mouse_pos.1;
            let mouse_moved = (dx * dx + dy * dy) > 64;

            let was_dimmed = self.idle_time > 30.0;
            if !actions.is_empty() || mouse_moved {
                self.idle_time = 0.0;
                self.active_mouse_pos = mouse_pos;
                if was_dimmed {
                    self.needs_draw = true;
                }
            } else {
                self.idle_time += dt;
            }

            let dim_fade_active = self.idle_time > 30.0 && self.idle_time < 32.0;
            let is_dimmed = self.idle_time > 30.0;

            if is_dimmed && !self.dimmed_gc_done {
                if self.keybinds.is_open() {
                    self.keybinds.close();
                }
                if self.details.is_open() {
                    self.details.close();
                }
                self.art_cache.clear();
                self.fonts.clear_textures();
                self.dimmed_gc_done = true;
            } else if !is_dimmed {
                self.dimmed_gc_done = false;
            }

            // OLED Orbit / Pixel Shift calculation
            if self.keybinds.pixel_shift_enabled {
                let t = self.start_instant.elapsed().as_secs_f64();
                let theta = (t / 180.0) * 2.0 * PI;
                let new_sx = (theta.cos() * 2.0) as i32;
                let new_sy = (theta.sin() * 2.0) as i32;
                if new_sx != self.shift_x || new_sy != self.shift_y {
                    self.shift_x = new_sx;
                    self.shift_y = new_sy;
                    self.needs_draw = true;
                }
            } else {
                self.shift_x = 0;
                self.shift_y = 0;
            }

            for act in &actions {
                self.handle_action(*act);
            }

            let grid_dirty = self.grid.update(dt);
            let sidebar_dirty = self.sidebar.update(dt);
            self.details.update(dt);

            let anim_sidebar = !self.sidebar.alpha_tw.done || sidebar_dirty;
            let anim_details = self.details.is_open() && !self.details.alpha_tw.done;
            let anim_scroll = (self.grid.scroll_y - self.grid.target_y).abs() > 0.5;

            let needs_render = self.needs_draw
                || grid_dirty
                || anim_sidebar
                || anim_details
                || anim_scroll
                || self.loading
                || self.details.is_open()
                || self.keybinds.is_open()
                || dim_fade_active;

            let is_active = needs_render
                || self.input.has_focus
                || self.input.mouse_in_window
                || !actions.is_empty()
                || mouse_moved
                || (self.idle_time < 3.0);

            let current_tick_fps = if is_active {
                self.target_fps
            } else {
                self.idle_fps
            };

            if needs_render {
                self.canvas
                    .set_draw_color(Color::RGB(config::BG.0, config::BG.1, config::BG.2));
                self.canvas.clear();

                let (sx, sy) = if self.keybinds.pixel_shift_enabled {
                    (self.shift_x, self.shift_y)
                } else {
                    (0, 0)
                };

                self.grid.draw(
                    &mut self.canvas,
                    &self.tc,
                    &mut self.fonts,
                    &mut self.art_cache,
                    sx,
                    sy,
                );
                self.sidebar.draw(
                    &mut self.canvas,
                    &self.tc,
                    &mut self.fonts,
                    &mut self.art_cache,
                    self.input.mouse_pos,
                    sx,
                    sy,
                );
                self.details.draw(
                    &mut self.canvas,
                    &self.tc,
                    &mut self.fonts,
                    &mut self.art_cache,
                    &mut self.icons,
                    sx,
                    sy,
                );
                self.keybinds.draw(
                    &mut self.canvas,
                    &self.tc,
                    &mut self.fonts,
                    &mut self.icons,
                    sx,
                    sy,
                );

                if self.loading {
                    draw_loading(
                        &mut self.canvas,
                        &self.tc,
                        &mut self.fonts,
                        self.win_size,
                        &self.status,
                    );
                } else if !self.status.is_empty() {
                    draw_status(
                        &mut self.canvas,
                        &self.tc,
                        &mut self.fonts,
                        self.win_size,
                        &self.status,
                    );
                }

                if is_dimmed {
                    let dim_alpha = (((self.idle_time - 30.0) * 90.0) as i32).clamp(0, 180) as u8;
                    self.canvas.set_blend_mode(BlendMode::Blend);
                    self.canvas.set_draw_color(Color::RGBA(0, 0, 0, dim_alpha));
                    let _ = self
                        .canvas
                        .fill_rect(Rect::new(0, 0, self.win_size.0, self.win_size.1));
                }

                self.canvas.present();
                self.needs_draw = false;
            }

            let target_frame_dur = Duration::from_secs_f64(1.0 / (current_tick_fps.max(1) as f64));
            let elapsed = now.elapsed();
            if elapsed < target_frame_dur {
                thread::sleep(target_frame_dur - elapsed);
            }
        }
    }
}

fn main() {
    setup_env_and_signals();

    if env::args().any(|a| a == "--dry-run") {
        dry_run();
        return;
    }

    let sdl = sdl2::init().expect("Failed to initialize SDL2");
    let _image = sdl2::image::init(InitFlag::PNG | InitFlag::JPG | InitFlag::WEBP)
        .expect("Failed to initialize SDL2_image");
    let ttf = sdl2::ttf::init().expect("Failed to initialize SDL2_ttf");

    let mut app = App::new(&sdl, &ttf).expect("Failed to initialize corpse-launcher App");
    app.run();
}
