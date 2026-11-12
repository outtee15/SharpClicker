// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicU8, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::thread;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, Instant};

use device_query::{DeviceQuery, DeviceState, Keycode};
use eframe::egui::{self, Color32, RichText, Rounding, Stroke, Vec2};
use enigo::{Button, Direction, Enigo, Mouse, Settings};

const ACCENT: Color32 = Color32::from_rgb(99, 102, 241);
const GREEN: Color32 = Color32::from_rgb(34, 197, 94);
const RED: Color32 = Color32::from_rgb(239, 68, 68);
const AMBER: Color32 = Color32::from_rgb(245, 158, 11);
const PANEL: Color32 = Color32::from_rgb(24, 24, 30);
const CARD: Color32 = Color32::from_rgb(34, 34, 44);

/// With the backlog guard on, clicks are never sent faster than this (ms).
const MIN_SAFE_MS: u64 = 10;

/// Everything that gets saved between launches.
#[derive(Clone)]
struct Config {
    hours: u64,
    minutes: u64,
    seconds: u64,
    millis: u64,
    always_on_top: bool,
    backlog_guard: bool,
    button_idx: u8,
    hotkey: Keycode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hours: 0,
            minutes: 0,
            seconds: 0,
            millis: 1,            // default: 1 millisecond
            always_on_top: false, // default: off
            backlog_guard: true,
            button_idx: 0,
            hotkey: Keycode::F6,
        }
    }
}

impl Config {
    /// %APPDATA%\SharpClicker\settings.txt
    fn path() -> Option<PathBuf> {
        std::env::var_os("APPDATA")
            .map(|p| PathBuf::from(p).join("SharpClicker").join("settings.txt"))
    }

    /// Loads saved settings; anything missing or invalid falls back to the default.
    fn load() -> Self {
        let mut cfg = Config::default();
        let Some(path) = Self::path() else { return cfg };
        let Ok(text) = std::fs::read_to_string(path) else { return cfg };

        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "hours" => {
                    if let Ok(n) = v.parse::<u64>() {
                        cfg.hours = n.min(99);
                    }
                }
                "minutes" => {
                    if let Ok(n) = v.parse::<u64>() {
                        cfg.minutes = n.min(59);
                    }
                }
                "seconds" => {
                    if let Ok(n) = v.parse::<u64>() {
                        cfg.seconds = n.min(59);
                    }
                }
                "millis" => {
                    if let Ok(n) = v.parse::<u64>() {
                        cfg.millis = n.min(999);
                    }
                }
                "always_on_top" => cfg.always_on_top = v == "true",
                "backlog_guard" => cfg.backlog_guard = v == "true",
                "button" => {
                    if let Ok(n) = v.parse::<u8>() {
                        cfg.button_idx = n.min(2);
                    }
                }
                "hotkey" => {
                    if let Ok(key) = Keycode::from_str(v) {
                        if key != Keycode::Delete {
                            cfg.hotkey = key;
                        }
                    }
                }
                _ => {}
            }
        }
        cfg
    }

    fn to_text(&self) -> String {
        format!(
            "hours={}\nminutes={}\nseconds={}\nmillis={}\nalways_on_top={}\nbacklog_guard={}\nbutton={}\nhotkey={:?}\n",
            self.hours,
            self.minutes,
            self.seconds,
            self.millis,
            self.always_on_top,
            self.backlog_guard,
            self.button_idx,
            self.hotkey,
        )
    }

    fn write_text(text: &str) {
        if let Some(path) = Self::path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, text);
        }
    }
}

struct Shared {
    running: AtomicBool,
    interval_ms: AtomicU64,
    button: AtomicU8, // 0 = left, 1 = right, 2 = middle
    clicks: AtomicU64,
    hotkey: Mutex<Keycode>,
    capturing: AtomicBool, // true while waiting for the user to press a new hotkey
    backlog_guard: AtomicBool, // caps click rate so the OS input queue can't pile up

    // This app's own window rectangle in physical screen pixels.
    // The clicker never clicks while the cursor is inside it.
    rect_valid: AtomicBool,
    rx0: AtomicI32,
    ry0: AtomicI32,
    rx1: AtomicI32,
    ry1: AtomicI32,
    paused_over_app: AtomicBool,
}

fn cursor_over_app(shared: &Shared, device: &DeviceState) -> bool {
    if !shared.rect_valid.load(Relaxed) {
        return false;
    }
    let (x, y) = device.get_mouse().coords;
    x >= shared.rx0.load(Relaxed)
        && x <= shared.rx1.load(Relaxed)
        && y >= shared.ry0.load(Relaxed)
        && y <= shared.ry1.load(Relaxed)
}

fn spawn_clicker(shared: Arc<Shared>) {
    thread::spawn(move || {
        let mut enigo = Enigo::new(&Settings::default()).expect("failed to init input");
        let device = DeviceState::new();
        let mut next = Instant::now();
        loop {
            if !shared.running.load(Relaxed) {
                shared.paused_over_app.store(false, Relaxed);
                thread::sleep(Duration::from_millis(10));
                next = Instant::now();
                continue;
            }

            let now = Instant::now();
            if now < next {
                // Plain sleep: no busy-spinning, so no wasted CPU / lag.
                thread::sleep(next - now);
                continue;
            }

            let mut ms = shared.interval_ms.load(Relaxed).max(1);
            if shared.backlog_guard.load(Relaxed) && ms < MIN_SAFE_MS {
                ms = MIN_SAFE_MS;
            }
            let iv = Duration::from_millis(ms);

            // Safety: never click on this app's own window.
            if cursor_over_app(&shared, &device) {
                shared.paused_over_app.store(true, Relaxed);
                next = Instant::now() + Duration::from_millis(20);
                continue;
            }
            shared.paused_over_app.store(false, Relaxed);

            let btn = match shared.button.load(Relaxed) {
                1 => Button::Right,
                2 => Button::Middle,
                _ => Button::Left,
            };
            let _ = enigo.button(btn, Direction::Click);
            shared.clicks.fetch_add(1, Relaxed);

            next += iv;
            let after = Instant::now();
            if next < after {
                next = after + iv; // never burst-catch-up if we fell behind
            }
        }
    });
}

/// Global hotkey thread:
///  - DELETE always stops clicking and kills the whole process
///  - the chosen toggle key starts/stops clicking
///  - captures a new toggle key when asked
fn spawn_hotkey(shared: Arc<Shared>) {
    thread::spawn(move || {
        let device = DeviceState::new();
        let mut was_down = false;
        loop {
            let keys = device.get_keys();

            // Kill switch: works from any window, even while rebinding.
            if keys.contains(&Keycode::Delete) {
                shared.running.store(false, Relaxed);
                std::process::exit(0);
            }

            if shared.capturing.load(Relaxed) {
                if let Some(&k) = keys.first() {
                    *shared.hotkey.lock().unwrap() = k;
                    shared.capturing.store(false, Relaxed);
                    was_down = true; // ignore this press so it doesn't toggle immediately
                }
            } else {
                let hk = *shared.hotkey.lock().unwrap();
                let down = keys.contains(&hk);
                if down && !was_down {
                    let now = shared.running.load(Relaxed);
                    shared.running.store(!now, Relaxed);
                }
                was_down = down;
            }
            thread::sleep(Duration::from_millis(20));
        }
    });
}

struct App {
    hours: u64,
    minutes: u64,
    seconds: u64,
    millis: u64,
    always_on_top: bool,
    backlog_guard: bool,
    button_idx: u8,
    shared: Arc<Shared>,
    // autosave bookkeeping
    pending: String,
    pending_since: Instant,
    last_saved: String,
}

impl Drop for App {
    fn drop(&mut self) {
        self.shared.running.store(false, Relaxed);
    }
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, cfg: Config) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = PANEL;
        visuals.window_fill = PANEL;
        visuals.widgets.inactive.bg_fill = CARD;
        visuals.widgets.inactive.weak_bg_fill = CARD;
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 48, 62);
        visuals.widgets.active.weak_bg_fill = ACCENT;
        visuals.selection.bg_fill = ACCENT;
        visuals.widgets.inactive.rounding = Rounding::same(8.0);
        visuals.widgets.hovered.rounding = Rounding::same(8.0);
        visuals.widgets.active.rounding = Rounding::same(8.0);
        cc.egui_ctx.set_visuals(visuals);

        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        cc.egui_ctx.set_style(style);

        let shared = Arc::new(Shared {
            running: AtomicBool::new(false),
            interval_ms: AtomicU64::new(1),
            button: AtomicU8::new(cfg.button_idx),
            clicks: AtomicU64::new(0),
            hotkey: Mutex::new(cfg.hotkey),
            capturing: AtomicBool::new(false),
            backlog_guard: AtomicBool::new(cfg.backlog_guard),
            rect_valid: AtomicBool::new(false),
            rx0: AtomicI32::new(0),
            ry0: AtomicI32::new(0),
            rx1: AtomicI32::new(0),
            ry1: AtomicI32::new(0),
            paused_over_app: AtomicBool::new(false),
        });
        spawn_clicker(shared.clone());
        spawn_hotkey(shared.clone());

        let text = cfg.to_text();
        Self {
            hours: cfg.hours,
            minutes: cfg.minutes,
            seconds: cfg.seconds,
            millis: cfg.millis,
            always_on_top: cfg.always_on_top,
            backlog_guard: cfg.backlog_guard,
            button_idx: cfg.button_idx,
            shared,
            pending: text.clone(),
            pending_since: Instant::now(),
            last_saved: text,
        }
    }

    fn total_ms(&self) -> u64 {
        self.hours * 3_600_000 + self.minutes * 60_000 + self.seconds * 1_000 + self.millis
    }

    fn time_field(ui: &mut egui::Ui, label: &str, value: &mut u64, max: u64) {
        ui.vertical(|ui| {
            ui.label(RichText::new(label).size(12.0).color(Color32::GRAY));
            ui.add_sized(
                [64.0, 32.0],
                egui::DragValue::new(value).range(0..=max).speed(0.2),
            );
        });
    }

    /// Saves settings shortly after they stop changing (or immediately if `force`).
    fn autosave(&mut self, force: bool) {
        let cfg = Config {
            hours: self.hours,
            minutes: self.minutes,
            seconds: self.seconds,
            millis: self.millis,
            always_on_top: self.always_on_top,
            backlog_guard: self.backlog_guard,
            button_idx: self.button_idx,
            hotkey: *self.shared.hotkey.lock().unwrap(),
        };
        let text = cfg.to_text();
        if text != self.pending {
            self.pending = text;
            self.pending_since = Instant::now();
        }
        if self.pending != self.last_saved
            && (force || self.pending_since.elapsed() > Duration::from_millis(400))
        {
            Config::write_text(&self.pending);
            self.last_saved = self.pending.clone();
        }
    }

    /// Publish this window's screen rectangle so the clicker can avoid it.
    fn publish_window_rect(&self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        let (rect, minimized) = ctx.input(|i| {
            let v = i.viewport();
            (v.outer_rect.or(v.inner_rect), v.minimized.unwrap_or(false))
        });
        match rect {
            Some(r) if !minimized => {
                let pad = 8.0;
                self.shared.rx0.store(((r.min.x * ppp) - pad) as i32, Relaxed);
                self.shared.ry0.store(((r.min.y * ppp) - pad) as i32, Relaxed);
                self.shared.rx1.store(((r.max.x * ppp) + pad) as i32, Relaxed);
                self.shared.ry1.store(((r.max.y * ppp) + pad) as i32, Relaxed);
                self.shared.rect_valid.store(true, Relaxed);
            }
            _ => self.shared.rect_valid.store(false, Relaxed),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Closing the window always stops clicking first.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.shared.running.store(false, Relaxed);
            self.autosave(true);
        }

        self.publish_window_rect(ctx);

        // Never allow a 0 ms interval.
        if self.total_ms() == 0 {
            self.millis = 1;
        }
        self.shared.interval_ms.store(self.total_ms(), Relaxed);
        self.shared.button.store(self.button_idx, Relaxed);
        self.shared.backlog_guard.store(self.backlog_guard, Relaxed);
        self.autosave(false);

        let running = self.shared.running.load(Relaxed);
        let capturing = self.shared.capturing.load(Relaxed);
        let paused = self.shared.paused_over_app.load(Relaxed);
        let hotkey_name = format!("{:?}", *self.shared.hotkey.lock().unwrap());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("SharpClicker").size(24.0).strong().color(Color32::WHITE));
            ui.label(
                RichText::new(format!(
                    "{hotkey_name} = start / stop  •  Del = close & kill"
                ))
                .size(12.0)
                .color(Color32::GRAY),
            );
            ui.add_space(6.0);

            // Interval card
            egui::Frame::none()
                .fill(CARD)
                .rounding(Rounding::same(12.0))
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Click interval").strong());
                    ui.horizontal(|ui| {
                        Self::time_field(ui, "Hours", &mut self.hours, 99);
                        Self::time_field(ui, "Minutes", &mut self.minutes, 59);
                        Self::time_field(ui, "Seconds", &mut self.seconds, 59);
                        Self::time_field(ui, "Millis", &mut self.millis, 999);
                    });
                });

            // Mouse button card
            egui::Frame::none()
                .fill(CARD)
                .rounding(Rounding::same(12.0))
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Mouse button").strong());
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.button_idx, 0, "Left");
                        ui.selectable_value(&mut self.button_idx, 1, "Right");
                        ui.selectable_value(&mut self.button_idx, 2, "Middle");
                    });
                });

            // Settings card
            egui::Frame::none()
                .fill(CARD)
                .rounding(Rounding::same(12.0))
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Settings").strong());

                    // Always on top (default: off)
                    if ui
                        .checkbox(&mut self.always_on_top, "Show above other apps")
                        .changed()
                    {
                        let level = if self.always_on_top {
                            egui::WindowLevel::AlwaysOnTop
                        } else {
                            egui::WindowLevel::Normal
                        };
                        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
                    }

                    // Backlog guard
                    ui.checkbox(&mut self.backlog_guard, "Backlog guard (stops leftover clicks)")
                        .on_hover_text("Caps clicking at 100/sec (10 ms). Faster rates can flood the target app's input queue, so clicks keep landing after you stop.");

                    // Hotkey rebinding
                    ui.horizontal(|ui| {
                        ui.label("Toggle key:");
                        let label = if capturing {
                            "Press a key… (click to cancel)".to_string()
                        } else {
                            format!("{hotkey_name}  (click to change)")
                        };
                        if ui.button(label).clicked() {
                            self.shared.capturing.store(!capturing, Relaxed);
                        }
                    });
                });

            ui.add_space(2.0);

            // Single start / stop toggle button
            let (text, color) = if running {
                ("■  Stop", RED)
            } else {
                ("▶  Start", GREEN)
            };
            let btn = egui::Button::new(RichText::new(text).size(18.0).strong().color(Color32::WHITE))
                .fill(color)
                .stroke(Stroke::NONE)
                .rounding(Rounding::same(10.0))
                .min_size(Vec2::new(ui.available_width(), 44.0));
            if ui.add(btn).clicked() {
                self.shared.running.store(!running, Relaxed);
            }

            ui.horizontal(|ui| {
                let (dot, status) = if running && paused {
                    (AMBER, "Paused – move the cursor off this window")
                } else if running {
                    (GREEN, "Running")
                } else {
                    (Color32::GRAY, "Idle")
                };
                ui.label(RichText::new("●").color(dot));
                ui.label(status);
            });
            if self.backlog_guard && self.total_ms() < MIN_SAFE_MS {
                ui.label(
                    RichText::new(format!("Effective interval: {MIN_SAFE_MS} ms (backlog guard on)"))
                        .size(12.0)
                        .color(AMBER),
                );
            }
            ui.label(
                RichText::new(format!("Clicks: {}", self.shared.clicks.load(Relaxed)))
                    .color(Color32::GRAY),
            );
        });

        // Keep UI in sync with the global hotkey and click counter.
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

/// Loads src/icon.ico for the window / taskbar icon.
/// Reads the ICO directory by hand, picks the largest image and decodes it as PNG
/// (any PNG colour type works). Never panics: returns None if anything is wrong,
/// and the window just uses the default icon.
fn load_icon() -> Option<egui::IconData> {
    let bytes: &[u8] = include_bytes!("icon.ico");
    if bytes.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;

    // (width, data offset, data length) of the largest entry
    let mut best: Option<(u32, usize, usize)> = None;
    for i in 0..count {
        let e = 6 + i * 16;
        if bytes.len() < e + 16 {
            break;
        }
        let w = if bytes[e] == 0 { 256 } else { bytes[e] as u32 };
        let len = u32::from_le_bytes([bytes[e + 8], bytes[e + 9], bytes[e + 10], bytes[e + 11]]) as usize;
        let off = u32::from_le_bytes([bytes[e + 12], bytes[e + 13], bytes[e + 14], bytes[e + 15]]) as usize;
        if best.map_or(true, |(bw, _, _)| w > bw) {
            best = Some((w, off, len));
        }
    }
    let (_, off, len) = best?;
    let data = bytes.get(off..off.checked_add(len)?)?;

    let img = if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        image::load_from_memory_with_format(data, image::ImageFormat::Png).ok()?
    } else {
        image::load_from_memory_with_format(bytes, image::ImageFormat::Ico).ok()?
    }
    .into_rgba8();

    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

fn main() {
    let cfg = Config::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([360.0, 600.0])
        .with_resizable(false)
        .with_title("SharpClicker");
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    if cfg.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let result = eframe::run_native(
        "SharpClicker",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, cfg)))),
    );
    // Force the whole process (including background threads) to exit.
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}