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

const MIN_SAFE_MS: u64 = 10;

#[derive(Clone)]
struct Config {
    hours: u64,
    minutes: u64,
    seconds: u64,
    millis: u64,
    always_on_top: bool,
    backlog_guard: bool,
    guard_ms: u64,
    button_idx: u8,
    hotkey: Keycode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hours: 0,
            minutes: 0,
            seconds: 0,
            millis: 1,
            always_on_top: false,
            backlog_guard: true,
            guard_ms: MIN_SAFE_MS,
            button_idx: 0,
            hotkey: Keycode::F6,
        }
    }
}

impl Config {

    fn path() -> Option<PathBuf> {
        std::env::var_os("APPDATA")
            .map(|p| PathBuf::from(p).join("SharpClicker").join("settings.txt"))
    }

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
                "guard_ms" => {
                    if let Ok(n) = v.parse::<u64>() {
                        cfg.guard_ms = n.clamp(1, 100);
                    }
                }
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
            "hours={}\nminutes={}\nseconds={}\nmillis={}\nalways_on_top={}\nbacklog_guard={}\nguard_ms={}\nbutton={}\nhotkey={:?}\n",
            self.hours,
            self.minutes,
            self.seconds,
            self.millis,
            self.always_on_top,
            self.backlog_guard,
            self.guard_ms,
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
    button: AtomicU8,
    clicks: AtomicU64,
    hotkey: Mutex<Keycode>,
    capturing: AtomicBool,
    backlog_guard: AtomicBool,
    guard_ms: AtomicU64,

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

                thread::sleep(next - now);
                continue;
            }

            let mut ms = shared.interval_ms.load(Relaxed).max(1);
            let guard = shared.guard_ms.load(Relaxed).max(1);
            if shared.backlog_guard.load(Relaxed) && ms < guard {
                ms = guard;
            }
            let iv = Duration::from_millis(ms);

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
                next = after + iv;
            }
        }
    });
}

fn spawn_hotkey(shared: Arc<Shared>) {
    thread::spawn(move || {
        let device = DeviceState::new();
        let mut was_down = false;
        loop {
            let keys = device.get_keys();

            if keys.contains(&Keycode::Delete) {
                shared.running.store(false, Relaxed);
                std::process::exit(0);
            }

            if shared.capturing.load(Relaxed) {
                if let Some(&k) = keys.first() {
                    *shared.hotkey.lock().unwrap() = k;
                    shared.capturing.store(false, Relaxed);
                    was_down = true;
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
    guard_ms: u64,
    button_idx: u8,
    shared: Arc<Shared>,

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
            guard_ms: AtomicU64::new(cfg.guard_ms),
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
            guard_ms: cfg.guard_ms,
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

    fn autosave(&mut self, force: bool) {
        let cfg = Config {
            hours: self.hours,
            minutes: self.minutes,
            seconds: self.seconds,
            millis: self.millis,
            always_on_top: self.always_on_top,
            backlog_guard: self.backlog_guard,
            guard_ms: self.guard_ms,
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

        if ctx.input(|i| i.viewport().close_requested()) {
            self.shared.running.store(false, Relaxed);
            self.autosave(true);
        }

        self.publish_window_rect(ctx);

        if self.total_ms() == 0 {
            self.millis = 1;
        }
        self.shared.interval_ms.store(self.total_ms(), Relaxed);
        self.shared.button.store(self.button_idx, Relaxed);
        self.shared.backlog_guard.store(self.backlog_guard, Relaxed);
        self.shared.guard_ms.store(self.guard_ms, Relaxed);
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

            egui::Frame::none()
                .fill(CARD)
                .rounding(Rounding::same(12.0))
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Settings").strong());

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

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.backlog_guard, "Backlog guard")
                            .on_hover_text("Stops leftover clicks after you press stop by never sending clicks faster than the app under the cursor can handle.");
                        ui.add_enabled(
                            self.backlog_guard,
                            egui::DragValue::new(&mut self.guard_ms).range(1..=100).suffix(" ms"),
                        )
                        .on_hover_text("Minimum time between clicks. Lower it until leftover clicks start appearing after you stop, then raise it a little.");
                    });

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
            if self.backlog_guard && self.total_ms() < self.guard_ms {
                ui.label(
                    RichText::new(format!("Effective interval: {} ms (backlog guard on)", self.guard_ms))
                        .size(12.0)
                        .color(AMBER),
                );
            }
            ui.label(
                RichText::new(format!("Clicks: {}", self.shared.clicks.load(Relaxed)))
                    .color(Color32::GRAY),
            );
        });

        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn load_icon() -> Option<egui::IconData> {
    let bytes: &[u8] = include_bytes!("icon.ico");
    if bytes.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;

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

    std::process::exit(if result.is_ok() { 0 } else { 1 });
}