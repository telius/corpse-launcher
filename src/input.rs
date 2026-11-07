use sdl2::event::{Event, WindowEvent};
use sdl2::joystick::{HatState, Joystick};
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::{EventPump, JoystickSubsystem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NavUp,
    NavDown,
    NavLeft,
    NavRight,
    Confirm,
    Back,
    Details,
    Hide,
    ShowHidden,
    PageLeft,
    PageRight,
    Refresh,
    Quit,
    Help,
    TogglePixelshift,
    ToggleFps,
    ToggleAnimations,
    MouseClick,
    MouseRightClick,
    ScrollUp,
    ScrollDown,
    MouseHover,
    FocusGained,
}

const STICK_DEADZONE: f32 = 0.25;
const STICK_REPEAT_INIT: f64 = 0.25;
const STICK_REPEAT_RATE: f64 = 0.08;

const HAT_REPEAT_INIT: f64 = 0.22;
const HAT_REPEAT_RATE: f64 = 0.06;

const KEY_REPEAT_INIT: f64 = 0.22;
const KEY_REPEAT_RATE: f64 = 0.05;

#[derive(Clone, Default)]
struct RepeatState {
    action: Option<Action>,
    timer: f64,
    phase: u8,
}

pub struct InputHandler {
    joy_sub: Option<JoystickSubsystem>,
    controller: Option<Joystick>,
    hat_state: (i32, i32),
    stick_dir: (i32, i32),
    hat_repeat: RepeatState,
    stick_repeat: RepeatState,
    held_key: Option<Keycode>,
    key_repeat: RepeatState,
    pub mouse_pos: (i32, i32),
    pub has_focus: bool,
    pub mouse_in_window: bool,
}

impl InputHandler {
    pub fn new(joy_sub: Option<JoystickSubsystem>) -> Self {
        let mut handler = Self {
            joy_sub,
            controller: None,
            hat_state: (0, 0),
            stick_dir: (0, 0),
            hat_repeat: RepeatState::default(),
            stick_repeat: RepeatState::default(),
            held_key: None,
            key_repeat: RepeatState::default(),
            mouse_pos: (0, 0),
            has_focus: true,
            mouse_in_window: true,
        };
        handler.init_controller();
        handler
    }

    fn init_controller(&mut self) {
        let Some(ref js) = self.joy_sub else {
            return;
        };
        match js.num_joysticks() {
            Ok(count) if count > 0 => match js.open(0) {
                Ok(c) => {
                    println!(
                        "[input] Controller: {} ({} buttons, {} axes)",
                        c.name(),
                        c.num_buttons(),
                        c.num_axes()
                    );
                    self.controller = Some(c);
                }
                Err(e) => {
                    println!("[input] Joystick initialization error: {}", e);
                    self.controller = None;
                }
            },
            _ => {
                self.controller = None;
                println!("[input] No controller detected. Keyboard + mouse navigation.");
            }
        }
    }

    fn stick_direction(&self) -> (i32, i32) {
        let Some(ref c) = self.controller else {
            return (0, 0);
        };
        let ax = c.axis(0).unwrap_or(0) as f32 / 32768.0;
        let ay = c.axis(1).unwrap_or(0) as f32 / 32768.0;
        let x = if ax.abs() < STICK_DEADZONE {
            0
        } else if ax > 0.0 {
            1
        } else {
            -1
        };
        let y = if ay.abs() < STICK_DEADZONE {
            0
        } else if ay > 0.0 {
            1
        } else {
            -1
        };
        (x, y)
    }

    fn action_from_stick(&self) -> Option<Action> {
        let (sx, sy) = self.stick_dir;
        if sy == -1 {
            Some(Action::NavUp)
        } else if sy == 1 {
            Some(Action::NavDown)
        } else if sx == -1 {
            Some(Action::NavLeft)
        } else if sx == 1 {
            Some(Action::NavRight)
        } else {
            None
        }
    }

    fn action_from_hat(&self) -> Option<Action> {
        match self.hat_state {
            (0, 1) => Some(Action::NavUp),
            (0, -1) => Some(Action::NavDown),
            (-1, 0) => Some(Action::NavLeft),
            (1, 0) => Some(Action::NavRight),
            _ => None,
        }
    }

    fn map_key(key: Keycode) -> Option<Action> {
        match key {
            Keycode::Up | Keycode::W => Some(Action::NavUp),
            Keycode::Down | Keycode::S => Some(Action::NavDown),
            Keycode::Left | Keycode::A => Some(Action::NavLeft),
            Keycode::Right | Keycode::D => Some(Action::NavRight),
            Keycode::Return | Keycode::KpEnter | Keycode::Space => Some(Action::Confirm),
            Keycode::Escape => Some(Action::Back),
            Keycode::F5 => Some(Action::Refresh),
            Keycode::F1 => Some(Action::Help),
            Keycode::H => Some(Action::Hide),
            Keycode::Tab => Some(Action::ShowHidden),
            Keycode::I => Some(Action::Details),
            Keycode::PageUp => Some(Action::PageLeft),
            Keycode::PageDown => Some(Action::PageRight),
            Keycode::P => Some(Action::TogglePixelshift),
            Keycode::F => Some(Action::ToggleFps),
            Keycode::N => Some(Action::ToggleAnimations),
            _ => None,
        }
    }

    fn is_repeatable_key(key: Keycode) -> bool {
        matches!(
            key,
            Keycode::Up
                | Keycode::Down
                | Keycode::Left
                | Keycode::Right
                | Keycode::W
                | Keycode::S
                | Keycode::A
                | Keycode::D
        )
    }

    fn map_button(btn: u8) -> Option<Action> {
        match btn {
            0 => Some(Action::Confirm),
            1 => Some(Action::Back),
            2 => Some(Action::Details),
            3 => Some(Action::Details),
            4 => Some(Action::ShowHidden),
            6 => Some(Action::Refresh),
            9 => Some(Action::PageLeft),
            10 => Some(Action::PageRight),
            11 => Some(Action::Hide),
            _ => None,
        }
    }

    pub fn poll(&mut self, event_pump: &mut EventPump, dt: f64) -> Vec<Action> {
        let mut actions = Vec::new();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => actions.push(Action::Quit),

                Event::KeyDown {
                    keycode: Some(key),
                    repeat: false,
                    ..
                } => {
                    if let Some(act) = Self::map_key(key) {
                        actions.push(act);
                        if Self::is_repeatable_key(key) {
                            self.held_key = Some(key);
                            self.key_repeat = RepeatState {
                                action: Some(act),
                                timer: 0.0,
                                phase: 0,
                            };
                        }
                    }
                }

                Event::KeyUp {
                    keycode: Some(key), ..
                } => {
                    if self.held_key == Some(key) {
                        self.held_key = None;
                        self.key_repeat = RepeatState::default();
                    }
                }

                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => match mouse_btn {
                    MouseButton::Left => {
                        self.mouse_pos = (x, y);
                        actions.push(Action::MouseClick);
                    }
                    MouseButton::Right => {
                        self.mouse_pos = (x, y);
                        actions.push(Action::MouseRightClick);
                    }
                    _ => {}
                },

                Event::MouseWheel { y, .. } => {
                    if y > 0 {
                        actions.push(Action::ScrollUp);
                    } else if y < 0 {
                        actions.push(Action::ScrollDown);
                    }
                }

                Event::MouseMotion { x, y, .. } => {
                    self.mouse_pos = (x, y);
                    actions.push(Action::MouseHover);
                }

                Event::JoyButtonDown { button_idx, .. } => {
                    if let Some(act) = Self::map_button(button_idx) {
                        actions.push(act);
                    }
                }

                Event::JoyHatMotion { state, .. } => {
                    self.hat_state = match state {
                        HatState::Up => (0, 1),
                        HatState::Down => (0, -1),
                        HatState::Left => (-1, 0),
                        HatState::Right => (1, 0),
                        _ => (0, 0),
                    };
                    self.hat_repeat = RepeatState::default();
                    if let Some(act) = self.action_from_hat() {
                        actions.push(act);
                        self.hat_repeat = RepeatState {
                            action: Some(act),
                            timer: 0.0,
                            phase: 0,
                        };
                    }
                }

                Event::JoyDeviceAdded { which, .. } => {
                    if self.controller.is_none() {
                        if let Some(ref js) = self.joy_sub {
                            if let Ok(c) = js.open(which) {
                                self.controller = Some(c);
                            }
                        }
                    }
                }

                Event::JoyDeviceRemoved { .. } => {
                    self.controller = None;
                }

                Event::Window { win_event, .. } => match win_event {
                    WindowEvent::FocusGained
                    | WindowEvent::Enter
                    | WindowEvent::Shown
                    | WindowEvent::Restored
                    | WindowEvent::Exposed => {
                        self.has_focus = true;
                        if win_event == WindowEvent::Enter {
                            self.mouse_in_window = true;
                        }
                        actions.push(Action::FocusGained);
                    }
                    WindowEvent::FocusLost => {
                        self.has_focus = false;
                        self.held_key = None;
                        self.key_repeat = RepeatState::default();
                    }
                    WindowEvent::Leave => {
                        self.mouse_in_window = false;
                    }
                    _ => {}
                },

                _ => {}
            }
        }

        // Keyboard repeat
        if self.held_key.is_some() {
            if let Some(act) = self.key_repeat.action {
                self.key_repeat.timer += dt;
                let threshold = if self.key_repeat.phase == 0 {
                    KEY_REPEAT_INIT
                } else {
                    KEY_REPEAT_RATE
                };
                if self.key_repeat.timer >= threshold {
                    self.key_repeat.timer = 0.0;
                    self.key_repeat.phase = 1;
                    actions.push(act);
                }
            }
        }

        // Stick continuous state
        let new_stick_dir = self.stick_direction();
        if new_stick_dir != self.stick_dir {
            self.stick_dir = new_stick_dir;
            self.stick_repeat = RepeatState::default();
            if let Some(act) = self.action_from_stick() {
                actions.push(act);
                self.stick_repeat = RepeatState {
                    action: Some(act),
                    timer: 0.0,
                    phase: 0,
                };
            }
        }

        // Hat repeat
        if let Some(act) = self.hat_repeat.action {
            self.hat_repeat.timer += dt;
            let threshold = if self.hat_repeat.phase == 0 {
                HAT_REPEAT_INIT
            } else {
                HAT_REPEAT_RATE
            };
            if self.hat_repeat.timer >= threshold {
                self.hat_repeat.timer = 0.0;
                self.hat_repeat.phase = 1;
                if self.action_from_hat() == Some(act) {
                    actions.push(act);
                } else {
                    self.hat_repeat = RepeatState::default();
                }
            }
        }

        // Stick repeat
        if let Some(act) = self.stick_repeat.action {
            self.stick_repeat.timer += dt;
            let threshold = if self.stick_repeat.phase == 0 {
                STICK_REPEAT_INIT
            } else {
                STICK_REPEAT_RATE
            };
            if self.stick_repeat.timer >= threshold {
                self.stick_repeat.timer = 0.0;
                self.stick_repeat.phase = 1;
                if self.action_from_stick() == Some(act) {
                    actions.push(act);
                } else {
                    self.stick_repeat = RepeatState::default();
                }
            }
        }

        actions
    }
}
