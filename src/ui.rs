//! The front panel, drawn by hand so the plugin and the standalone app look identical.

use crate::bank;
use crate::dsp::SoundEngine;
use crate::mp7;
use crate::params::{
    apply_kit, delay_division, delay_free_ms, fx_spec, internal_bpm, load_mp7_instrument, phaser_hz,
    set_sound_engine, sound_engine, spec, sync_division, voice_spec, FxParam, ParamAccess, ParamId,
    VoiceParam, DELAY_DIVISIONS, DELAY_MODE_NAMES, FILTER_HP_THRESHOLD, FX_BUS_NAMES, FX_BUS_SHORT,
    MP7_KIT_CHOICE, NUM_CH, NUM_FX_BUSES, NUM_KIT_CHOICES, NUM_STEPS, SYNC_NAMES, VOICE_LETTERS,
};
use crate::setup::{SetupHost, SetupUi};
use crate::state::{NUM_CHAIN, SharedState, NUM_PAD_MODES, PAD_ACCENT, PAD_LENGTH, PAD_MODE_NAMES, PAD_MULTI, PAD_NOTE, PAD_PROB};
use egui::{pos2, vec2, Align2, Color32, FontId, Id, Pos2, Rect, Response, Sense, Shape, Stroke};
use std::sync::atomic::Ordering::Relaxed;

/// Design width: 760 + 80 for the NUDGE / DRIFT columns.
pub const W: f32 = 840.0;
/// The right-hand column (nameplate, MIDI, module slots) moved right by this much.
const RIGHT_SHIFT: f32 = 80.0;
pub const H: f32 = 490.0;
/// The window opens this many times the design size (1344 x 784), as the person asked.
pub const START_SCALE: f32 = 1.6;

const BG: Color32 = Color32::from_rgb(10, 10, 11);
const CASE: Color32 = Color32::from_rgb(24, 25, 27);
const PANEL_TOP: Color32 = Color32::from_rgb(50, 62, 74);
const PANEL_BOT: Color32 = Color32::from_rgb(34, 43, 52);
const INK: Color32 = Color32::from_rgb(214, 220, 226);
const INK_DIM: Color32 = Color32::from_rgb(140, 152, 164);
const ORANGE_HI: Color32 = Color32::from_rgb(255, 178, 92);
const ORANGE_ON: Color32 = Color32::from_rgb(236, 132, 58);
const ORANGE_DEAD: Color32 = Color32::from_rgb(52, 32, 24);
const RED_LED: Color32 = Color32::from_rgb(255, 48, 36);
const GREEN_HI: Color32 = Color32::from_rgb(110, 240, 100);

pub struct UiState {
    pub open_module: Option<usize>,
    paint: Option<bool>,
    clipboard: Option<bank::ProgramPattern>,
    seen_hits: [u32; NUM_CH],
    lamp_time: [f64; NUM_CH],
    message: String,
    message_time: f64,
    rng: u64,
    chassis: Option<egui::TextureHandle>,
    pcb: Option<egui::TextureHandle>,
    /// The FX panel is open (or opening). The flip itself is fx_p, 0 = front, 1 = FX side.
    pub fx_open: bool,
    /// Holds the flip at one point of its travel (for pictures of the animation).
    pub fx_freeze: Option<f32>,
    pub fx_p: f32,
    last_time: f64,
    last_step: i32,
    step_time: f64,
    kick_time: f64,
    /// A LOAD / SAVE file dialog running on its own thread (see bank.rs).
    dialog: Option<bank::DialogTask>,
    /// The SETUP window (window size; audio and MIDI devices in the standalone app).
    pub setup: SetupUi,
    /// The CHAIN panel behind the name plate is open (or opening); chain_p is its flip,
    /// 0 = name plate, 1 = chain panel.
    pub chain_open: bool,
    pub chain_p: f32,
    /// Holds the CHAIN flip at one point (for pictures).
    pub chain_freeze: Option<f32>,
}

impl Default for UiState {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x1234_5678);
        UiState {
            open_module: None,
            paint: None,
            clipboard: None,
            seen_hits: [0; NUM_CH],
            lamp_time: [-10.0; NUM_CH],
            message: String::new(),
            message_time: -10.0,
            rng: seed | 1,
            chassis: None,
            pcb: None,
            fx_open: false,
            fx_freeze: None,
            fx_p: 0.0,
            last_time: 0.0,
            last_step: -1,
            step_time: -10.0,
            kick_time: -10.0,
            dialog: None,
            setup: SetupUi::default(),
            chain_open: false,
            chain_p: 0.0,
            chain_freeze: None,
        }
    }
}

impl UiState {
    /// 0..=32767, like the C library's rand() the original uses.
    fn rand15(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        ((x >> 33) & 0x7fff) as u32
    }
}

/// Maps design coordinates (840x490) to the screen. For the FX panel flip, the main plate
/// can be tilted around a horizontal hinge and seen in perspective, and made inactive
/// (live false) so nothing can be clicked while it moves.
#[derive(Clone, Copy)]
struct G {
    s: f32,
    o: Pos2,
    /// cos and sin of the panel's tilt around the hinge (1, 0 when flat).
    c: f32,
    sn: f32,
    live: bool,
    /// The hinge (design units): the main plate's, or the name plate's for the CHAIN panel.
    hx: f32,
    hy: f32,
}

/// The hinge of the fold-out panel: a horizontal line across the middle of the main plate.
const HINGE_X: f32 = 348.0;
const HINGE_Y: f32 = 251.0;
/// Viewing distance for the perspective of the tilting panel, in design units.
const EYE: f32 = 1600.0;

impl G {
    fn new(s: f32, o: Pos2) -> Self {
        G { s, o, c: 1.0, sn: 0.0, live: true, hx: HINGE_X, hy: HINGE_Y }
    }
    fn shifted(self, dx: f32) -> Self {
        G { o: pos2(self.o.x + dx * self.s, self.o.y), ..self }
    }
    /// The same panel tilted around the hinge (inactive while it moves).
    fn tilted(self, c: f32, sn: f32) -> Self {
        G { c, sn, live: false, ..self }
    }
    /// Tilted around another hinge (the name plate's, for the CHAIN panel).
    fn tilted_about(self, c: f32, sn: f32, hx: f32, hy: f32) -> Self {
        G { c, sn, live: false, hx, hy, ..self }
    }
    /// A point of the panel, w design units behind its surface, seen in perspective.
    fn p3(&self, x: f32, y: f32, w: f32) -> Pos2 {
        let d = y - self.hy;
        let yy = d * self.c - w * self.sn;
        let z = d * self.sn + w * self.c;
        let k = EYE / (EYE - z);
        pos2(self.o.x + (self.hx + (x - self.hx) * k) * self.s, self.o.y + (self.hy + yy * k) * self.s)
    }
    fn p(&self, x: f32, y: f32) -> Pos2 {
        if self.sn == 0.0 && self.c == 1.0 {
            pos2(self.o.x + x * self.s, self.o.y + y * self.s)
        } else {
            self.p3(x, y, 0.0)
        }
    }
    fn r(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(self.p(x0, y0), self.p(x1, y1))
    }
    fn f(&self, size: f32) -> FontId {
        FontId::proportional((size * self.s * self.c.abs()).max(0.5))
    }
    fn l(&self, w: f32) -> f32 {
        w * self.s * self.c.abs()
    }
    fn sense(&self, s: Sense) -> Sense {
        if self.live {
            s
        } else {
            Sense::hover()
        }
    }
}

pub fn draw(ctx: &egui::Context, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState, host: &mut dyn SetupHost) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(BG))
        .show(ctx, |ui| draw_in(ui, access, state, st, host));
    ctx.request_repaint_after(std::time::Duration::from_millis(30));
}

/// The whole panel inside a given area, scaled to fit it. `host` is where the panel lives
/// (plugin or standalone app): the SETUP window asks it for window sizes and devices.
pub fn draw_in(ui: &mut egui::Ui, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState, host: &mut dyn SetupHost) {
    let ctx = ui.ctx().clone();
    // A plugin window gets a fresh drawing context each time it is opened, while this state
    // lives on: pictures loaded into the previous context don't exist in the new one (the case
    // and the plate came out black). Load them again whenever the context is new.
    let marker = Id::new("ritmo6-textures-loaded");
    if !ctx.data(|d| d.get_temp::<bool>(marker).unwrap_or(false)) {
        st.chassis = None;
        st.pcb = None;
        ctx.data_mut(|d| d.insert_temp(marker, true));
    }
    ui.painter().rect_filled(ui.max_rect(), 0.0, BG);
    poll_dialog(st, access, state, ctx.input(|i| i.time));
    {
        {
            let avail = ui.max_rect();
            let s = (avail.width() / W).min(avail.height() / H).max(0.2);
            let o = avail.center() - vec2(W * s / 2.0, H * s / 2.0);
            let g = G::new(s, o);
            let now = ctx.input(|i| i.time);
            for ch in 0..NUM_CH {
                let h = state.hits[ch].load(Relaxed);
                if h != st.seen_hits[ch] {
                    st.seen_hits[ch] = h;
                    st.lamp_time[ch] = now;
                }
            }
            let cur = state.cur_step.load(Relaxed);
            if cur != st.last_step {
                st.last_step = cur;
                st.step_time = now;
            }
            // the fold-out FX panel: fx_p runs 0 -> 1 in FLIP_SECS
            let dt = (now - st.last_time).clamp(0.0, 0.1) as f32;
            st.last_time = now;
            match st.fx_freeze {
                Some(f) => st.fx_p = f.clamp(0.0, 1.0),
                None => {
                    let step = dt / FLIP_SECS;
                    st.fx_p = if st.fx_open { (st.fx_p + step).min(1.0) } else { (st.fx_p - step).max(0.0) };
                }
            }
            {
                let step = dt / FLIP_SECS;
                st.chain_p = match st.chain_freeze {
                    Some(f) => f.clamp(0.0, 1.0),
                    None if st.chain_open => (st.chain_p + step).min(1.0),
                    None => (st.chain_p - step).max(0.0),
                };
                if st.chain_p > 0.0 && st.chain_p < 1.0 {
                    ctx.request_repaint();
                }
            }
            draw_case(ui, g, st);
            if st.fx_p <= 0.0 {
                draw_main(ui, g, access, state, st, now);
            } else if st.fx_p >= 1.0 {
                draw_fx_panel(ui, g, access, state, st, now);
            } else {
                let th = flip_angle(st.fx_p, st.fx_open);
                let (c, sn) = (th.cos(), th.sin());
                let front = g.tilted(c, sn);
                draw_cavity(ui, g, front, st, state, now);
                if c >= 0.0 {
                    plate_face(ui, front, st);
                    draw_main(ui, front, access, state, st, now);
                } else {
                    // the back of the plate: its own upright coordinates, mirrored by the turn
                    let back = g.tilted(-c, -sn);
                    plate_face(ui, back, st);
                    draw_fx_panel(ui, back, access, state, st, now);
                }
                ctx.request_repaint();
            }
            let rg = g.shifted(RIGHT_SHIFT);
            if let Some(ch) = st.open_module {
                draw_module(ui, rg, access, state, st, ch);
            } else {
                draw_right(ui, rg, access, state, st);
            }
            if now - st.message_time < 3.0 && !st.message.is_empty() {
                ui.painter().text(
                    g.p(44.0, 470.0),
                    Align2::LEFT_CENTER,
                    &st.message,
                    g.f(10.0),
                    Color32::from_rgb(255, 200, 120),
                );
            }
            crate::setup::draw(&ctx, g.r(0.0, 0.0, W, H), g.s, &mut st.setup, host);
        }
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(30));
}

/// Open a file dialog on its own thread, one at a time (a second click while one is open does nothing).
fn start_dialog(st: &mut UiState, now: f64, open: impl FnOnce() -> bank::DialogTask) {
    if st.dialog.is_some() {
        say(st, Some("A file window is already open".into()), now);
        return;
    }
    st.dialog = Some(open());
}

/// Pick up a finished file dialog and apply it here, on the GUI thread.
fn poll_dialog(st: &mut UiState, access: &dyn ParamAccess, state: &SharedState, now: f64) {
    let Some(task) = &st.dialog else { return };
    let finished = match task.try_lock() {
        Ok(mut slot) => slot.take(),
        Err(_) => None,
    };
    if let Some(result) = finished {
        st.dialog = None;
        if let Some(done) = result {
            let m = bank::finish_dialog(done, access, state);
            say(st, Some(m), now);
        }
    }
}

fn say(st: &mut UiState, msg: Option<String>, now: f64) {
    if let Some(m) = msg {
        st.message = m;
        st.message_time = now;
    }
}

// ---------------------------------------------------------------- painting helpers

fn vgrad(ui: &egui::Ui, rect: Rect, top: Color32, bottom: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(rect.left_top(), top);
    m.colored_vertex(rect.right_top(), top);
    m.colored_vertex(rect.right_bottom(), bottom);
    m.colored_vertex(rect.left_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(m));
}

impl G {
    fn flat(&self) -> bool {
        self.c == 1.0 && self.sn == 0.0
    }
}

/// A filled box given in design coordinates: rounded when the plate is flat, its four
/// corners in perspective while it turns.
#[allow(clippy::too_many_arguments)]
fn fill(ui: &egui::Ui, g: G, x0: f32, y0: f32, x1: f32, y1: f32, round: f32, color: Color32) {
    if g.flat() {
        ui.painter().rect_filled(g.r(x0, y0, x1, y1), g.l(round), color);
    } else {
        let pts = vec![g.p(x0, y0), g.p(x1, y0), g.p(x1, y1), g.p(x0, y1)];
        ui.painter().add(Shape::convex_polygon(pts, color, Stroke::NONE));
    }
}

/// A vertical gradient box given in design coordinates (follows the perspective).
#[allow(clippy::too_many_arguments)]
fn vgrad_d(ui: &egui::Ui, g: G, x0: f32, y0: f32, x1: f32, y1: f32, top: Color32, bottom: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(g.p(x0, y0), top);
    m.colored_vertex(g.p(x1, y0), top);
    m.colored_vertex(g.p(x1, y1), bottom);
    m.colored_vertex(g.p(x0, y1), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(m));
}

fn text(ui: &egui::Ui, g: G, x: f32, y: f32, t: &str, size: f32, color: Color32) {
    ui.painter().text(g.p(x, y), Align2::CENTER_CENTER, t, g.f(size), color);
}

/// The 3D-rendered plastic chassis (tools/chassis.py, rendered with Blender at 2x).
const CHASSIS_PNG: &[u8] = include_bytes!("../assets/chassis.png");

fn chassis_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(CHASSIS_PNG).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    // draw this frame for sure: the plugin window only uploads new pictures with a frame it shows
    ctx.request_repaint();
    Some(ctx.load_texture("chassis", color, egui::TextureOptions::LINEAR))
}

fn draw_case(ui: &egui::Ui, g: G, st: &mut UiState) {
    if st.chassis.is_none() {
        st.chassis = chassis_texture(ui.ctx());
    }
    match &st.chassis {
        Some(tex) => {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            ui.painter().image(tex.id(), g.r(0.0, 0.0, W, H), uv, Color32::WHITE);
        }
        None => {
            ui.painter().rect_filled(g.r(8.0, 18.0, 752.0, 480.0), g.l(20.0), CASE);
            vgrad(ui, g.r(36.0, 40.0, 580.0, 462.0), PANEL_TOP, PANEL_BOT);
        }
    }
}

// ---------------------------------------------------------------- the fold-out FX panel

/// How long the plate takes to turn over, in seconds.
const FLIP_SECS: f32 = 0.65;
/// The main plate (the part that turns over), in design coordinates.
const PLATE: (f32, f32, f32, f32) = (34.0, 38.0, 662.0, 464.0);
/// The inside of the case under the plate (tools/pcb.py, rendered with Blender at 2x).
const PCB_PNG: &[u8] = include_bytes!("../assets/pcb.png");
/// The red bead on the board, in plate coordinates.
const PCB_LED: (f32, f32) = (594.0, 102.0);

fn png_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    // draw this frame for sure: the plugin window only uploads new pictures with a frame it shows
    ctx.request_repaint();
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

/// The plate's angle around the hinge for a point of the travel: a slow start as the latch
/// lets go, a quick swing, and a small settle as it lands on its stops.
fn flip_angle(p: f32, opening: bool) -> f32 {
    use std::f32::consts::{PI, TAU};
    let q = if opening { p } else { 1.0 - p };
    let e = if q < 0.5 { 4.0 * q * q * q } else { 1.0 - (-2.0 * q + 2.0).powi(3) / 2.0 };
    let settle = if q > 0.82 {
        let r = (q - 0.82) / 0.18;
        0.06 * (r * TAU).sin() * (1.0 - r)
    } else {
        0.0
    };
    let a = PI * e + settle;
    if opening {
        a
    } else {
        PI - a
    }
}

/// A part of a texture laid on the (possibly tilted) plate, in strips so the perspective holds.
#[allow(clippy::too_many_arguments)]
fn tex_on_plate(ui: &egui::Ui, g: G, tex: egui::TextureId, x0: f32, y0: f32, x1: f32, y1: f32, uv: Rect, tint: Color32) {
    let mut m = egui::Mesh::with_texture(tex);
    let n = 24u32;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let y = y0 + (y1 - y0) * t;
        let v = uv.min.y + (uv.max.y - uv.min.y) * t;
        m.vertices.push(egui::epaint::Vertex { pos: g.p(x0, y), uv: pos2(uv.min.x, v), color: tint });
        m.vertices.push(egui::epaint::Vertex { pos: g.p(x1, y), uv: pos2(uv.max.x, v), color: tint });
        if i > 0 {
            let b = 2 * i;
            m.add_triangle(b - 2, b - 1, b + 1);
            m.add_triangle(b - 2, b + 1, b);
        }
    }
    ui.painter().add(Shape::mesh(m));
}

/// What shows under the plate while it turns: the brown circuit board, its shadows and its
/// LED beating with the steps.
fn draw_cavity(ui: &egui::Ui, g: G, tilted: G, st: &mut UiState, state: &SharedState, now: f64) {
    if st.pcb.is_none() {
        st.pcb = png_texture(ui.ctx(), "pcb", PCB_PNG);
    }
    let (x0, y0, x1, y1) = PLATE;
    let p = ui.painter();
    let well = g.r(x0, y0, x1, y1);
    p.rect_filled(well, 0.0, Color32::from_rgb(20, 14, 10));
    if let Some(tex) = &st.pcb {
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        p.image(tex.id(), well, uv, Color32::WHITE);
    }
    // the case walls shade the board's edges
    let dark = Color32::from_black_alpha(170);
    let clear = Color32::from_black_alpha(0);
    vgrad(ui, g.r(x0, y0, x1, y0 + 26.0), dark, clear);
    vgrad(ui, g.r(x0, y1 - 18.0, x1, y1), clear, Color32::from_black_alpha(120));
    // the LED: a flash on every step while it plays, a slow breath when stopped
    let running = state.running.load(Relaxed) || state.host_driven.load(Relaxed);
    let glow = if running {
        (-(now - st.step_time) as f32 * 7.0).exp()
    } else {
        0.35 + 0.35 * ((now * 2.4).sin() as f32)
    };
    let led = g.p(PCB_LED.0, PCB_LED.1);
    let a = (glow.clamp(0.0, 1.0) * 255.0) as u8;
    p.circle_filled(led, g.l(12.0), Color32::from_rgba_unmultiplied(255, 40, 20, a / 5));
    p.circle_filled(led, g.l(6.0), Color32::from_rgba_unmultiplied(255, 60, 30, a / 3));
    p.circle_filled(led, g.l(3.2), Color32::from_rgba_unmultiplied(255, 120, 90, a));
    // the turning plate's shadow on the board
    let k = tilted.sn.abs();
    let off = vec2(g.l(10.0) * k, g.l(16.0) * k);
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let mut pts: Vec<Pos2> = corners.iter().map(|&(x, y)| tilted.p(x, y) + off).collect();
    if tilted.c < 0.0 {
        pts.reverse();
    }
    p.add(Shape::convex_polygon(pts, Color32::from_black_alpha((120.0 * k) as u8), Stroke::NONE));
}

/// The turning plate itself: its edges (it has a thickness), then its face, shaded by the
/// angle, then the hinge pins.
fn plate_face(ui: &egui::Ui, g: G, st: &UiState) {
    let (x0, y0, x1, y1) = PLATE;
    let p = ui.painter();
    let t = 8.0;
    let shade = 0.45 + 0.55 * g.c.abs();
    let edge = Color32::from_rgb((70.0 * shade) as u8, (72.0 * shade) as u8, (76.0 * shade) as u8);
    let quads = [
        [g.p3(x0, y0, 0.0), g.p3(x1, y0, 0.0), g.p3(x1, y0, t), g.p3(x0, y0, t)],
        [g.p3(x0, y1, 0.0), g.p3(x1, y1, 0.0), g.p3(x1, y1, t), g.p3(x0, y1, t)],
        [g.p3(x0, y0, 0.0), g.p3(x0, y1, 0.0), g.p3(x0, y1, t), g.p3(x0, y0, t)],
        [g.p3(x1, y0, 0.0), g.p3(x1, y1, 0.0), g.p3(x1, y1, t), g.p3(x1, y0, t)],
    ];
    for q in quads {
        let mut m = egui::Mesh::default();
        for v in q {
            m.colored_vertex(v, edge);
        }
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        p.add(Shape::mesh(m));
    }
    let v = (255.0 * shade) as u8;
    let tint = Color32::from_rgb(v, v, v);
    match &st.chassis {
        Some(tex) => {
            let uv = Rect::from_min_max(pos2(x0 / W, y0 / H), pos2(x1 / W, y1 / H));
            tex_on_plate(ui, g, tex.id(), x0, y0, x1, y1, uv, tint);
        }
        None => {
            let pts = vec![g.p(x0, y0), g.p(x1, y0), g.p(x1, y1), g.p(x0, y1)];
            p.add(Shape::convex_polygon(pts, Color32::from_rgb(73, 74, 77), Stroke::NONE));
        }
    }
    let rim = [g.p(x0, y0), g.p(x1, y0), g.p(x1, y1), g.p(x0, y1), g.p(x0, y0)];
    p.add(Shape::line(rim.to_vec(), Stroke::new(g.s, Color32::from_rgb(20, 20, 22))));
    // hinge pins at both ends of the turning axis
    for x in [x0 - 3.0, x1 - 5.0] {
        let r = Rect::from_min_max(g.p3(x, HINGE_Y, 0.0) - vec2(0.0, 4.0 * g.s), g.p3(x + 8.0, HINGE_Y, 0.0) + vec2(0.0, 4.0 * g.s));
        vgrad(ui, r, Color32::from_rgb(220, 222, 226), Color32::from_rgb(90, 92, 96));
    }
}

/// The FX button (bottom right of the plate), on both sides of it.
fn fx_button(ui: &mut egui::Ui, g: G, st: &mut UiState) {
    let r = g.r(604.0, 422.0, 650.0, 442.0);
    let resp = ui.interact(r, Id::new("fx_button"), g.sense(Sense::click()));
    fill(ui, g, 604.0, 424.0, 650.0, 444.0, 3.0, Color32::from_black_alpha(150));
    fill(ui, g, 604.0, 422.0, 650.0, 442.0, 3.0, Color32::from_rgb(20, 20, 22));
    let down = resp.is_pointer_button_down_on();
    let (top, bot) = if down {
        (Color32::from_rgb(170, 70, 30), Color32::from_rgb(200, 96, 44))
    } else if resp.hovered() {
        (Color32::from_rgb(250, 140, 70), Color32::from_rgb(200, 90, 40))
    } else {
        (Color32::from_rgb(232, 120, 56), Color32::from_rgb(180, 76, 30))
    };
    vgrad_d(ui, g, 607.0, 425.0, 647.0, 439.0, top, bot);
    text(ui, g, 618.0, 411.0, "FX", 12.0, ORANGE_HI);
    let lit = st.fx_open;
    ui.painter().circle_filled(g.p(640.0, 411.0), g.l(3.0), if lit { RED_LED } else { Color32::from_rgb(70, 20, 18) });
    let tip = if st.fx_open { "Close the FX panel" } else { "Open the FX panel: phaser, distortion, delay and spring reverb" };
    if resp.on_hover_text(tip).clicked() {
        st.fx_open = !st.fx_open;
    }
}

/// The back of the plate: the four shared effects, each with its own knobs, a bypass
/// footswitch and an output level. Voices reach them through their send knobs.
fn draw_fx_panel(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState, now: f64) {
    use FxParam::*;
    let fx = ParamId::Fx;
    const X: [f32; 4] = [46.0, 200.0, 354.0, 508.0];
    const SW: f32 = 142.0;
    let heads = [
        (Color32::from_rgb(222, 112, 40), Color32::from_rgb(30, 20, 14)),
        (Color32::from_rgb(28, 28, 30), Color32::from_rgb(236, 236, 236)),
        (Color32::from_rgb(110, 150, 120), Color32::from_rgb(18, 28, 20)),
        (Color32::from_rgb(86, 112, 150), Color32::from_rgb(236, 240, 246)),
    ];
    let titles = ["PHASER", "DISTORTION", "DELAY", "SPRING REVERB"];
    let knobs: [&[FxParam]; 4] = [
        &[PhRate, PhDepth, PhColor, PhMix, PhLevel],
        &[DsDrive, DsFilter, DsFat, DsMix, DsLevel],
        &[DlTime, DlFeedback, DlTone, DlAge, DlMod, DlMix, DlLevel],
        &[SpDecay, SpDamping, SpTension, SpDrive, SpMix, SpLevel, SpLowCut, SpHighCut],
    ];
    let bypass = [PhBypass, DsBypass, DlBypass, SpBypass];
    for k in 0..4 {
        let x0 = X[k];
        let x1 = x0 + SW;
        let mid = x0 + SW / 2.0;
        fill(ui, g, x0 - 1.0, 49.0, x1 + 1.0, 401.0, 7.0, Color32::from_rgb(12, 12, 13));
        fill(ui, g, x0, 50.0, x1, 400.0, 6.0, Color32::from_rgb(34, 36, 39));
        fill(ui, g, x0 + 5.0, 55.0, x1 - 5.0, 76.0, 3.0, heads[k].0);
        text(ui, g, mid, 65.5, titles[k], 9.5, heads[k].1);
        for (i, p) in knobs[k].iter().enumerate() {
            let x = x0 + 24.0 + 47.0 * (i % 3) as f32;
            let y = 102.0 + 48.0 * (i / 3) as f32;
            knob(ui, g, access, fx(*p), x, y, 12.5);
            text(ui, g, x, y + 20.0, fx_spec(*p).short, 6.5, INK);
        }
        // bypass footswitch with its LED (lit = the effect is in)
        let on = access.get(fx(bypass[k])) < 0.5;
        ui.painter().circle_filled(g.p(mid, 333.0), g.l(3.5), if on { RED_LED } else { Color32::from_rgb(70, 20, 18) });
        if on {
            ui.painter().circle_filled(g.p(mid, 333.0), g.l(7.0), RED_LED.gamma_multiply(0.2));
        }
        if round_button(ui, g, Id::new(("fx_bypass", k)), mid, 360.0, 13.0, None)
            .on_hover_text(format!("{}: {}", FX_BUS_NAMES[k], if on { "on (click to bypass)" } else { "bypassed (click to switch on)" }))
            .clicked()
        {
            access.set_gesture(fx(bypass[k]), if on { 1.0 } else { 0.0 });
        }
        text(ui, g, mid, 386.0, if on { "ON" } else { "BYPASS" }, 7.0, if on { INK } else { INK_DIM });
    }

    let lcd = |ui: &egui::Ui, cx: f32, y: f32, s: &str| {
        fill(ui, g, cx - 38.0, y - 9.0, cx + 38.0, y + 9.0, 2.0, Color32::from_rgb(30, 4, 4));
        text(ui, g, cx, y, s, 9.0, RED_LED);
    };
    // PHASER: the LFO lamp sweeps at the rate
    let (x0, mid) = (X[0], X[0] + SW / 2.0);
    let rate = access.get(fx(PhRate));
    let hz = phaser_hz(rate);
    let swing = 0.5 + 0.5 * ((now * hz as f64 * std::f64::consts::TAU).sin() as f32);
    ui.painter().circle_filled(g.p(x0 + 24.0, 262.0), g.l(4.0), Color32::from_rgba_unmultiplied(255, 150, 60, (60.0 + 195.0 * swing) as u8));
    text(ui, g, x0 + 24.0, 276.0, "LFO", 6.0, INK_DIM);
    lcd(ui, mid + 14.0, 262.0, &spec(fx(PhRate)).format(rate));
    text(ui, g, mid, 294.0, "4-STAGE  ·  COLOR = FEEDBACK", 5.5, INK_DIM);

    // DISTORTION
    let mid = X[1] + SW / 2.0;
    let drive = access.get(fx(DsDrive));
    let heat = drive.clamp(0.0, 1.0);
    ui.painter().circle_filled(g.p(X[1] + 24.0, 262.0), g.l(4.0), Color32::from_rgb((80.0 + 175.0 * heat) as u8, 30, 20));
    text(ui, g, X[1] + 24.0, 276.0, "CLIP", 6.0, INK_DIM);
    lcd(ui, mid + 14.0, 262.0, &spec(fx(DsDrive)).format(drive));
    text(ui, g, mid, 294.0, "OP-AMP CLIP  ·  4x OVERSAMPLED", 5.5, INK_DIM);

    // DELAY: MODE (TAPE / BBD / MOD) and SYNC / FREE with the time shown
    let x0 = X[2];
    let mid = x0 + SW / 2.0;
    let mode = access.get(fx(DlMode)).round().clamp(0.0, 2.0) as usize;
    for (i, name) in DELAY_MODE_NAMES.iter().enumerate() {
        let x = x0 + 24.0 + 47.0 * i as f32;
        let lit = (i == mode).then_some(Color32::from_rgb(140, 230, 150));
        if round_button(ui, g, Id::new(("dl_mode", i)), x, 248.0, 6.5, lit).on_hover_text(format!("Delay mode: {name}")).clicked() {
            access.set_gesture(fx(DlMode), i as f32);
        }
        text(ui, g, x, 260.0, name, 6.5, INK);
    }
    let sync = access.get(fx(DlSync)) >= 0.5;
    text(ui, g, x0 + 30.0, 281.0, "FREE", 6.5, INK);
    text(ui, g, x0 + 112.0, 281.0, "SYNC", 6.5, INK);
    if toggle_switch(ui, g, Id::new("dl_sync"), mid, 281.0, sync)
        .on_hover_text("SYNC: the time follows the tempo in note values  ·  FREE: milliseconds")
        .clicked()
    {
        access.set_gesture(fx(DlSync), if sync { 0.0 } else { 1.0 });
    }
    let tv = access.get(fx(DlTime));
    let shown = if sync { DELAY_DIVISIONS[delay_division(tv)].0.to_string() } else { format!("{:.0} ms", delay_free_ms(tv)) };
    lcd(ui, mid, 302.0, &shown);

    // SPRING: KICK the tank
    let x0 = X[3];
    let mid = x0 + SW / 2.0;
    let lamp = now - st.kick_time < 0.15;
    if round_button(ui, g, Id::new("spring_kick"), mid, 260.0, 14.0, lamp.then_some(ORANGE_HI))
        .on_hover_text("KICK: knock the spring tank (it crashes through the reverb)")
        .clicked()
    {
        state.spring_kick.store(true, Relaxed);
        st.kick_time = now;
    }
    text(ui, g, mid, 284.0, "KICK", 7.5, INK);
    text(ui, g, mid, 298.0, "3 SPRINGS  ·  TANK DRIVE", 5.5, INK_DIM);

    // bottom strip
    text(ui, g, 120.0, 428.0, "FX  BUSES", 13.0, ORANGE_HI);
    text(ui, g, 370.0, 424.0, "Turn up each voice's sends (PH  DS  DL  SP) in the module slots on the right.", 7.0, INK);
    text(ui, g, 370.0, 437.0, "The effects go to the main stereo output; the separate voice outputs stay dry.", 7.0, INK_DIM);
    fx_button(ui, g, st);
}

fn knob_face(ui: &egui::Ui, g: G, cx: f32, cy: f32, r: f32, norm: f32, pointer: Color32) {
    let p = ui.painter();
    let c = g.p(cx, cy);
    p.circle_filled(c + vec2(g.l(1.0), g.l(2.0)), g.l(r + 1.0), Color32::from_black_alpha(140));
    p.circle_filled(c, g.l(r), Color32::from_rgb(28, 30, 32));
    p.circle_filled(c, g.l(r * 0.82), Color32::from_rgb(150, 156, 162));
    p.circle_filled(c - vec2(g.l(r * 0.15), g.l(r * 0.2)), g.l(r * 0.55), Color32::from_rgb(196, 200, 205));
    p.circle_filled(c, g.l(r * 0.42), Color32::from_rgb(176, 181, 187));
    let a = (-135.0 + 270.0 * norm.clamp(0.0, 1.0)).to_radians();
    let dir = vec2(a.sin(), -a.cos());
    p.line_segment([c + dir * g.l(r * 0.25), c + dir * g.l(r * 0.92)], Stroke::new(g.l(2.0).max(1.0), pointer));
}

/// A rotary knob bound to a parameter (drag up/down, shift for fine, double-click to reset).
fn knob(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, id: ParamId, cx: f32, cy: f32, r: f32) -> Response {
    knob_with(ui, g, access, id, cx, cy, r, None)
}

/// A knob whose hover text is given (the MP7 module, where the knobs mean other things).
#[allow(clippy::too_many_arguments)]
fn knob_with(
    ui: &mut egui::Ui,
    g: G,
    access: &dyn ParamAccess,
    id: ParamId,
    cx: f32,
    cy: f32,
    r: f32,
    hover: Option<&dyn Fn(f32) -> String>,
) -> Response {
    let sp = spec(id);
    let rect = g.r(cx - r - 2.0, cy - r - 2.0, cx + r + 2.0, cy + r + 2.0);
    let resp = ui.interact(rect, Id::new(("knob", id)), g.sense(Sense::click_and_drag()));
    let value = access.get(id);
    let mut norm = sp.to_norm(value);
    if resp.drag_started() {
        access.begin(id);
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let d = resp.drag_delta();
        let delta = (-d.y + d.x) / g.l(if fine { 600.0 } else { 150.0 });
        norm = (norm + delta).clamp(0.0, 1.0);
        access.set(id, sp.from_norm(norm));
    }
    if resp.drag_stopped() {
        access.end(id);
    }
    if resp.double_clicked() {
        access.set_gesture(id, sp.default);
        norm = sp.to_norm(sp.default);
    }
    if resp.hovered() {
        let delta = ui.input(|i| i.smooth_scroll_delta.y);
        if delta != 0.0 {
            norm = (norm + delta / 600.0).clamp(0.0, 1.0);
            access.set_gesture(id, sp.from_norm(norm));
        }
    }
    knob_face(ui, g, cx, cy, r, norm, Color32::from_rgb(20, 20, 22));
    match hover {
        Some(h) => {
            let t = h(access.get(id));
            resp.on_hover_text(t)
        }
        None => {
            let shown = sp.format(access.get(id));
            resp.on_hover_text(format!("{}: {}", sp.name, shown))
        }
    }
}

fn round_button(ui: &mut egui::Ui, g: G, id: Id, cx: f32, cy: f32, r: f32, lit: Option<Color32>) -> Response {
    let rect = g.r(cx - r, cy - r, cx + r, cy + r);
    let resp = ui.interact(rect, id, g.sense(Sense::click()));
    let p = ui.painter();
    let c = g.p(cx, cy);
    p.circle_filled(c + vec2(0.0, g.l(1.5)), g.l(r + 1.0), Color32::from_black_alpha(150));
    p.circle_filled(c, g.l(r), Color32::from_rgb(20, 21, 23));
    let face = match lit {
        Some(col) => col,
        None if resp.is_pointer_button_down_on() => Color32::from_rgb(120, 124, 130),
        None if resp.hovered() => Color32::from_rgb(176, 180, 186),
        None => Color32::from_rgb(150, 154, 160),
    };
    p.circle_filled(c, g.l(r * 0.74), face);
    p.circle_filled(c - vec2(g.l(r * 0.18), g.l(r * 0.22)), g.l(r * 0.32), face.gamma_multiply(1.25));
    if let Some(col) = lit {
        p.circle_filled(c, g.l(r * 1.25), col.gamma_multiply(0.18));
    }
    resp
}

/// A three-position lever (0 left, 1 middle, 2 right).
fn three_way_switch(ui: &mut egui::Ui, g: G, id: Id, cx: f32, cy: f32, pos: usize) -> Response {
    let rect = g.r(cx - 19.0, cy - 6.0, cx + 19.0, cy + 6.0);
    let resp = ui.interact(rect, id, g.sense(Sense::click()));
    fill(ui, g, cx - 19.0, cy - 6.0, cx + 19.0, cy + 6.0, 6.0, Color32::from_rgb(14, 15, 16));
    let p = ui.painter();
    for k in 0..3 {
        p.circle_filled(g.p(cx - 12.0 + 12.0 * k as f32, cy), g.l(1.2), Color32::from_rgb(70, 72, 76));
    }
    let kx = cx - 12.0 + 12.0 * pos.min(2) as f32;
    p.circle_filled(g.p(kx, cy), g.l(5.0), Color32::from_rgb(196, 200, 206));
    p.circle_filled(g.p(kx - 1.0, cy - 1.5), g.l(2.0), Color32::WHITE);
    resp
}

fn toggle_switch(ui: &mut egui::Ui, g: G, id: Id, cx: f32, cy: f32, on: bool) -> Response {
    let rect = g.r(cx - 14.0, cy - 6.0, cx + 14.0, cy + 6.0);
    let resp = ui.interact(rect, id, g.sense(Sense::click()));
    fill(ui, g, cx - 14.0, cy - 6.0, cx + 14.0, cy + 6.0, 6.0, Color32::from_rgb(14, 15, 16));
    let p = ui.painter();
    let kx = if on { cx + 7.0 } else { cx - 7.0 };
    p.circle_filled(g.p(kx, cy), g.l(5.0), Color32::from_rgb(196, 200, 206));
    p.circle_filled(g.p(kx - 1.0, cy - 1.5), g.l(2.0), Color32::WHITE);
    resp
}

/// Horizontal fader with the level wedge above it.
fn hfader(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, id: ParamId, x0: f32, x1: f32, y: f32, wedge: Color32) {
    let sp = spec(id);
    let rect = g.r(x0 - 6.0, y - 18.0, x1 + 6.0, y + 8.0);
    let resp = ui.interact(rect, Id::new(("fader", id)), g.sense(Sense::click_and_drag()));
    let set_from_pointer = |pos: Pos2| {
        let n = ((pos.x - g.p(x0, 0.0).x) / g.l(x1 - x0)).clamp(0.0, 1.0);
        access.set(id, sp.from_norm(n));
    };
    if resp.drag_started() || resp.clicked() {
        access.begin(id);
        if let Some(pos) = resp.interact_pointer_pos() {
            set_from_pointer(pos);
        }
        if resp.clicked() {
            access.end(id);
        }
    }
    if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            set_from_pointer(pos);
        }
    }
    if resp.drag_stopped() {
        access.end(id);
    }
    if resp.double_clicked() {
        access.set_gesture(id, sp.default);
    }
    let n = sp.to_norm(access.get(id));
    let p = ui.painter();
    // wedge
    let base = y - 8.0;
    let full = [g.p(x0, base), g.p(x1, base), g.p(x1, base - 8.0)];
    p.add(Shape::convex_polygon(full.to_vec(), Color32::from_rgb(70, 76, 84), Stroke::NONE));
    let xn = x0 + (x1 - x0) * n;
    let lit = [g.p(x0, base), g.p(xn, base), g.p(xn, base - 8.0 * n)];
    if n > 0.01 {
        p.add(Shape::convex_polygon(lit.to_vec(), wedge, Stroke::NONE));
    }
    // track
    fill(ui, g, x0, y - 1.5, x1, y + 1.5, 1.0, Color32::from_rgb(8, 9, 10));
    // cap
    vgrad_d(ui, g, xn - 5.0, y - 7.0, xn + 5.0, y + 7.0, Color32::from_rgb(235, 236, 238), Color32::from_rgb(120, 124, 130));
    ui.painter().line_segment([g.p(xn, y - 6.0), g.p(xn, y + 6.0)], Stroke::new(g.l(1.5), Color32::from_rgb(30, 30, 30)));
    resp.on_hover_text(format!("{}: {:.0}%", sp.name, n * 100.0));
}

// ---------------------------------------------------------------- main panel

pub fn row_y(i: usize) -> f32 {
    86.0 + i as f32 * 42.0
}
const GRID_X0: f32 = 236.0;
const CELL_W: f32 = 17.5;
fn col_x(step: usize) -> f32 {
    GRID_X0 + CELL_W * (step as f32 + 0.5)
}

fn draw_main(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState, now: f64) {
    let prog = state.program();
    let len = state.length(prog);
    let cur = state.cur_step.load(Relaxed);

    // header labels
    text(ui, g, 138.0, 56.0, "PAN", 9.0, INK);
    text(ui, g, 172.0, 56.0, "TRIG", 9.0, INK);
    text(ui, g, 206.0, 56.0, "MUTE", 9.0, INK);
    for s in 0..NUM_STEPS {
        let on = cur == s as i32;
        let col = if on { ORANGE_HI } else if s < len { INK } else { INK_DIM };
        text(ui, g, col_x(s), 56.0, &(s + 1).to_string(), 8.5, col);
        text(ui, g, col_x(s), 316.0, &(s + 1).to_string(), 8.5, col);
    }

    for ch in 0..NUM_CH {
        let y = row_y(ch);
        text(ui, g, 46.0, y, VOICE_LETTERS[ch], 12.0, INK);
        hfader(ui, g, access, ParamId::Level(ch), 62.0, 112.0, y + 4.0, Color32::from_rgb(200, 206, 212));
        knob(ui, g, access, ParamId::Pan(ch), 138.0, y, 9.0);
        let lamp = now - st.lamp_time[ch] < 0.12;
        if round_button(ui, g, Id::new(("trig", ch)), 172.0, y, 8.0, lamp.then_some(ORANGE_HI))
            .on_hover_text(format!("Play voice {}", VOICE_LETTERS[ch]))
            .clicked()
        {
            state.trig_req.fetch_or(1 << ch, Relaxed);
        }
        let muted = access.get(ParamId::Mute(ch)) >= 0.5;
        if round_button(ui, g, Id::new(("mute", ch)), 206.0, y, 8.0, muted.then_some(RED_LED))
            .on_hover_text("MUTE: takes the voice (and its effects) out of the Mix. Its own separate output keeps playing, so you can send it to its own track without doubling it.")
            .clicked()
        {
            access.set_gesture(ParamId::Mute(ch), if muted { 0.0 } else { 1.0 });
        }
        draw_row(ui, g, state, st, prog, ch);
        // A: NUDGE (up to half a step early or late), B: this voice's share of SOUND DRIFT
        knob(ui, g, access, ParamId::Voice(ch, VoiceParam::Nudge), 604.0, y, 9.0);
        knob(ui, g, access, ParamId::Voice(ch, VoiceParam::DriftAmount), 638.0, y, 9.0);
        // ONE (solo) buttons
        let solo = state.solo.load(Relaxed);
        let soloed = solo & (1 << ch) != 0;
        if round_button(ui, g, Id::new(("solo", ch)), 548.0, y, 8.0, soloed.then_some(Color32::from_rgb(255, 214, 90)))
            .on_hover_text(format!("ONE: play only row {} (click again to add/remove)", VOICE_LETTERS[ch]))
            .clicked()
        {
            state.solo.fetch_xor(1 << ch, Relaxed);
        }
    }
    if !ui.input(|i| i.pointer.primary_down()) {
        st.paint = None;
    }
    // ALL + ONE bracket
    let solo_any = state.solo.load(Relaxed) != 0;
    if round_button(ui, g, Id::new("solo_all"), 548.0, 56.0, 8.0, (!solo_any).then_some(Color32::from_rgb(255, 214, 90)))
        .on_hover_text("ALL: every row plays")
        .clicked()
    {
        state.solo.store(0, Relaxed);
    }
    text(ui, g, 568.0, 56.0, "ALL", 8.0, INK);
    let p = ui.painter();
    let br = Stroke::new(g.l(1.0), INK_DIM);
    p.line_segment([g.p(558.0, row_y(0)), g.p(562.0, row_y(0))], br);
    p.line_segment([g.p(562.0, row_y(0)), g.p(562.0, row_y(5))], br);
    p.line_segment([g.p(558.0, row_y(5)), g.p(562.0, row_y(5))], br);
    text(ui, g, 571.0, row_y(2) + 21.0, "ONE", 7.5, INK);
    text(ui, g, 604.0, 52.0, "NUDGE", 7.0, INK);
    text(ui, g, 604.0, 61.0, "EARLY  LATE", 4.5, INK_DIM);
    text(ui, g, 638.0, 52.0, "DRIFT", 7.0, INK);
    text(ui, g, 638.0, 61.0, "AMOUNT", 4.5, INK_DIM);
    let p = ui.painter();
    p.line_segment([g.p(621.0, 70.0), g.p(621.0, 302.0)], Stroke::new(g.l(1.0), Color32::from_rgb(40, 41, 44)));
    chain_button(ui, g, state, st);
    fx_button(ui, g, st);

    // master volume
    hfader(ui, g, access, ParamId::Master, 62.0, 112.0, 340.0, Color32::from_rgb(230, 52, 40));
    text(ui, g, 87.0, 356.0, "MASTER VOL", 8.5, INK);

    // start/stop + keyboard control
    let running = state.running.load(Relaxed);
    if toggle_switch(ui, g, Id::new("run"), 87.0, 396.0, running)
        .on_hover_text("Start / stop the sequencer (space bar in the standalone app)")
        .clicked()
    {
        state.running.store(!running, Relaxed);
    }
    let host = state.host_driven.load(Relaxed);
    ui.painter().circle_filled(g.p(110.0, 396.0), g.l(3.0), if running || host { RED_LED } else { Color32::from_rgb(70, 20, 18) });
    text(ui, g, 87.0, 412.0, "STOP / START", 8.5, INK);
    let kb = state.keyb_ctrl.load(Relaxed);
    if toggle_switch(ui, g, Id::new("keyb"), 87.0, 436.0, kb)
        .on_hover_text("KEYB CTRL: MIDI keys 42-57 pick programs, 58+ start/stop")
        .clicked()
    {
        state.keyb_ctrl.store(!kb, Relaxed);
    }
    text(ui, g, 87.0, 451.0, "KEYB CTRL", 7.5, INK);

    // SOUND selector: MINI (first-day engine), ORIGINAL (decoded model) or MP7 (Mini Pops 7 style)
    let engine = sound_engine(access);
    let pos = match engine {
        SoundEngine::Mini => 0,
        SoundEngine::Original => 1,
        SoundEngine::Mp7 => 2,
    };
    text(ui, g, 140.0, 321.0, ["SOUND: MINI", "SOUND: ORIG", "SOUND: MP7"][pos], 7.5, ORANGE_HI);
    let r = three_way_switch(ui, g, Id::new("sound"), 140.0, 334.0, pos).on_hover_text(
        "Sound engine (click a position): MINI = the punchy first-day sound, ORIG = the model decoded \
         from the plugin, MP7 = the very analog Mini Pops 7 style instruments (bass drum, bongos, conga, \
         claves, cowbell, guiro, quijada...). Each voice's module picks its MP7 instrument; turning MP7 \
         on gives every voice the sound of its instrument.",
    );
    if r.clicked() {
        let next = match r.interact_pointer_pos() {
            Some(p) => {
                let x = (p.x - g.p(140.0, 0.0).x) / g.l(1.0);
                if x < -6.0 {
                    0
                } else if x > 6.0 {
                    2
                } else {
                    1
                }
            }
            None => (pos + 1) % 3,
        };
        let next = if next == pos { (pos + 1) % 3 } else { next };
        let e = [SoundEngine::Mini, SoundEngine::Original, SoundEngine::Mp7][next];
        set_sound_engine(access, e);
        let msg = match e {
            SoundEngine::Mini => "MINI sound",
            SoundEngine::Original => "ORIGINAL sound",
            SoundEngine::Mp7 => "MP7 sound: every voice plays its MP7 instrument (open a module to choose it)",
        };
        say(st, Some(msg.into()), now);
    }

    // TEMPO DRIFT / SOUND DRIFT (Minipop-style wobble)
    // (placed under the STEPS row, as in the person's mock-up)
    knob(ui, g, access, ParamId::TempoDrift, 248.0, 431.0, 10.0);
    text(ui, g, 248.0, 451.0, "TEMPO DRIFT", 7.5, INK);
    text(ui, g, 233.0, 442.0, "LATE", 5.5, INK_DIM);
    text(ui, g, 264.0, 442.0, "EARLY", 5.5, INK_DIM);
    knob(ui, g, access, ParamId::SoundDrift, 330.0, 431.0, 10.0);
    text(ui, g, 330.0, 451.0, "SOUND DRIFT", 7.5, INK);
    knob(ui, g, access, ParamId::Swing, 412.0, 431.0, 10.0);
    text(ui, g, 412.0, 451.0, "SWING", 7.5, INK);
    knob(ui, g, access, ParamId::Accent, 494.0, 431.0, 10.0)
        .on_hover_text(format!("ACCENT: how much louder and hotter the accented steps play ({})", spec(ParamId::Accent).format(access.get(ParamId::Accent))));
    text(ui, g, 494.0, 451.0, "ACCENT", 7.5, INK);

    pad_mode_selector(ui, g, state);

    // tempo knob with its 1..10 V scale
    let (tx, ty) = (170.0, 384.0);
    let tsp = spec(ParamId::Tempo);
    for n in 1..=10 {
        let a = (-135.0 + 270.0 * tsp.to_norm(n as f32)).to_radians();
        text(ui, g, tx + 28.0 * a.sin(), ty - 28.0 * a.cos(), &n.to_string(), 8.0, INK);
    }
    knob(ui, g, access, ParamId::Tempo, tx, ty, 18.0);
    let sync = access.get(ParamId::Sync) >= 0.5;
    let tempo = access.get(ParamId::Tempo);
    let readout = if sync {
        format!("÷ {}", SYNC_NAMES[sync_division(tempo)])
    } else {
        format!("{:.0} BPM", internal_bpm(tempo))
    };
    text(ui, g, tx, 418.0, &readout, 9.0, ORANGE_HI);
    if toggle_switch(ui, g, Id::new("sync"), tx, 437.0, sync)
        .on_hover_text("INTER: own tempo  ·  MIDI: follow the host, knob = divider")
        .clicked()
    {
        access.set_gesture(ParamId::Sync, if sync { 0.0 } else { 1.0 });
    }
    text(ui, g, tx - 26.0, 437.0, "INTER", 7.0, INK);
    text(ui, g, tx + 25.0, 437.0, "MIDI", 7.0, INK);
    text(ui, g, tx, 452.0, "TEMPO / DIVIDER", 7.5, INK);

    // PROGRAM row
    for i in 0..16 {
        let lit = (i == prog).then_some(Color32::from_rgb(255, 236, 170));
        if round_button(ui, g, Id::new(("prog", i)), col_x(i), 340.0, 6.5, lit)
            .on_hover_text(format!("Program {}", i + 1))
            .clicked()
        {
            state.program.store(i as u8, Relaxed);
        }
    }
    text(ui, g, 376.0, 356.0, "PROGRAM  ·  KEYB CTRL", 8.5, INK);
    // STEPS row
    for i in 0..16 {
        let lit = if i + 1 == len {
            Some(ORANGE_ON)
        } else if i < len {
            Some(Color32::from_rgb(120, 88, 60))
        } else {
            None
        };
        if round_button(ui, g, Id::new(("len", i)), col_x(i), 384.0, 6.5, lit)
            .on_hover_text(format!("Pattern length: {} steps, for every voice (LENGTH mode sets one voice)", i + 1))
            .clicked()
        {
            state.set_length(prog, i + 1);
        }
    }
    text(ui, g, 376.0, 400.0, "STEPS", 8.5, INK);
    let p = ui.painter();
    let line = Stroke::new(g.l(1.0), INK_DIM);
    p.line_segment([g.p(244.0, 350.0), g.p(340.0, 350.0)], line);
    p.line_segment([g.p(412.0, 350.0), g.p(508.0, 350.0)], line);
    p.line_segment([g.p(244.0, 394.0), g.p(356.0, 394.0)], line);
    p.line_segment([g.p(396.0, 394.0), g.p(508.0, 394.0)], line);

    // program tools
    let tools = [("RESET PRG", 336.0), ("COPY PRG", 366.0), ("PASTE PRG", 396.0), ("RANDOM", 426.0)];
    for (k, (label, y)) in tools.iter().enumerate() {
        let r = round_button(ui, g, Id::new(("tool", k)), 548.0, *y, 8.0, None);
        text(ui, g, 548.0, y + 14.0, label, 7.0, INK);
        if r.clicked() {
            match k {
                0 => {
                    state.clear_program(prog);
                    say(st, Some(format!("Program {} cleared", prog + 1)), now);
                }
                1 => {
                    st.clipboard = Some(bank::read_program(state, prog));
                    say(st, Some(format!("Program {} copied", prog + 1)), now);
                }
                2 => {
                    if let Some(p) = st.clipboard {
                        bank::write_program(state, prog, &p);
                        say(st, Some(format!("Pasted into program {}", prog + 1)), now);
                    } else {
                        say(st, Some("Copy a program first".into()), now);
                    }
                }
                _ => {
                    randomize_program(state, st, prog);
                    say(st, Some(format!("Program {} randomized", prog + 1)), now);
                }
            }
        }
    }
}

/// RANDOM, as the original sequencer (OCET00 0x10001600): every cell of the current program
/// becomes floor((rand() % 4) * rand() / 32768), and anything above 1 counts as off.
/// That switches a cell on with probability 5/24 (about 21%).
fn randomize_program(state: &SharedState, st: &mut UiState, prog: usize) {
    for c in 0..NUM_CH {
        let mut m = 0u16;
        for s in 0..NUM_STEPS {
            let a = st.rand15() % 4;
            let u = st.rand15() as f32 / 32768.0;
            let v = (a as f32 * u).floor() as u32;
            if v == 1 {
                m |= 1 << s;
            }
        }
        state.steps[prog][c].store(m, Relaxed);
        state.multi[prog][c].store(0, Relaxed);
        state.accent[prog][c].store(0, Relaxed);
        state.prob[prog][c].store(0, Relaxed);
    }
}

/// PAD MODE: what a click on a pad does. A small red display with arrows (like the kit and MIDI
/// displays); a click on the display itself also steps to the next mode.
fn pad_mode_selector(ui: &mut egui::Ui, g: G, state: &SharedState) {
    let mode = state.pad_mode.load(Relaxed).min(NUM_PAD_MODES - 1);
    let hint = [
        "NOTE: click or drag to switch steps on and off",
        "MULTI: each click gives 1, 2, 3, 4 hits inside the step, then off",
        "ACCENT: click to accent a step (red bar); the ACCENT knob sets how strong",
        "PROB: each click sets the chance of the step: 100, 75, 50, 25 %",
        "LENGTH: click a voice's last step to give it its own loop length",
    ][mode as usize];
    text(ui, g, 200.0, 321.0, "PAD MODE", 7.5, INK);
    let lcd = g.r(182.0, 327.0, 218.0, 341.0);
    ui.painter().rect_filled(lcd, g.l(2.0), Color32::from_rgb(30, 4, 4));
    text(ui, g, 200.0, 334.0, PAD_MODE_NAMES[mode as usize], 7.5, RED_LED);
    let tri = |x: f32, dir: f32| vec![g.p(x - 3.5 * dir, 334.0), g.p(x + 3.5 * dir, 330.0), g.p(x + 3.5 * dir, 338.0)];
    ui.painter().add(Shape::convex_polygon(tri(176.0, 1.0), INK, Stroke::NONE));
    ui.painter().add(Shape::convex_polygon(tri(224.0, -1.0).into_iter().rev().collect(), INK, Stroke::NONE));
    let dn = ui.interact(g.r(169.0, 327.0, 181.0, 341.0), Id::new("pad_dn"), g.sense(Sense::click()));
    let up = ui.interact(g.r(219.0, 327.0, 231.0, 341.0), Id::new("pad_up"), g.sense(Sense::click()));
    let mid = ui.interact(lcd, Id::new("pad_lcd"), g.sense(Sense::click()));
    let n = NUM_PAD_MODES;
    if dn.on_hover_text(hint).clicked() {
        state.pad_mode.store((mode + n - 1) % n, Relaxed);
    } else if up.on_hover_text(hint).clicked() || mid.on_hover_text(hint).clicked() {
        state.pad_mode.store((mode + 1) % n, Relaxed);
    }
}

fn draw_row(ui: &mut egui::Ui, g: G, state: &SharedState, st: &mut UiState, prog: usize, ch: usize) {
    let y = row_y(ch);
    let len = state.voice_length(prog, ch);
    let cur = state.voice_step[ch].load(Relaxed) as i32;
    let mode = state.pad_mode.load(Relaxed);
    let bar = g.r(GRID_X0 - 3.0, y - 10.0, GRID_X0 + CELL_W * 16.0 + 3.0, y + 10.0);
    fill(ui, g, GRID_X0 - 4.0, y - 11.0, GRID_X0 + CELL_W * 16.0 + 4.0, y + 11.0, 2.0, Color32::from_rgb(16, 12, 10));
    let off = (Color32::from_rgb(214, 104, 50), Color32::from_rgb(168, 70, 30));
    for s in 0..NUM_STEPS {
        let x0 = GRID_X0 + CELL_W * s as f32;
        let cell = g.r(x0 + 0.8, y - 9.0, x0 + CELL_W - 0.8, y + 9.0);
        let on = state.step_on(prog, ch, s);
        let hits = state.hits_at(prog, ch, s);
        let chance = if on { state.prob_at(prog, ch, s) } else { 100 };
        let accent = on && state.accent_on(prog, ch, s);
        // Subdivided steps light green: faint for 2 hits, brightest for 4.
        let green = match hits {
            2 => Some((Color32::from_rgb(70, 150, 70), Color32::from_rgb(28, 88, 30))),
            3 => Some((Color32::from_rgb(120, 210, 100), Color32::from_rgb(45, 145, 45))),
            4 => Some((Color32::from_rgb(200, 255, 170), GREEN_HI)),
            _ => None,
        };
        let outside = s >= len;
        let (top, bot) = if outside {
            (ORANGE_DEAD, Color32::from_rgb(36, 22, 16))
        } else if let Some(gc) = green {
            gc
        } else if on {
            (Color32::from_rgb(255, 236, 170), ORANGE_HI)
        } else {
            off
        };
        vgrad_d(ui, g, x0 + 0.8, y - 9.0, x0 + CELL_W - 0.8, y + 9.0, top, bot);
        // PROB: the pad is lit only up to its chance; the top stays unlit
        if on && chance < 100 && !outside {
            let lit_top = y + 9.0 - 18.0 * chance as f32 / 100.0;
            vgrad_d(ui, g, x0 + 0.8, y - 9.0, x0 + CELL_W - 0.8, lit_top, Color32::from_rgb(132, 56, 26), Color32::from_rgb(104, 42, 18));
            ui.painter().line_segment([g.p(x0 + 1.5, lit_top), g.p(x0 + CELL_W - 1.5, lit_top)], Stroke::new(g.l(0.6), Color32::from_rgb(255, 240, 200)));
        }
        let lamp_y = if chance < 100 { (y + 9.0 - 9.0 * chance as f32 / 100.0).max(y) } else { y };
        if on && hits <= 1 {
            fill(ui, g, x0 + 3.0, lamp_y - 2.0, x0 + CELL_W - 3.0, lamp_y + 2.0, 1.0, Color32::WHITE);
            if !outside {
                fill(ui, g, x0 - 0.7, y - 10.5, x0 + CELL_W + 0.7, y + 10.5, 2.0, Color32::from_rgba_unmultiplied(255, 220, 120, 40));
            }
        } else if hits >= 2 {
            // one small light per hit inside the step
            let glow = 20 + 25 * hits as u8;
            if !outside {
                fill(ui, g, x0 - 0.7, y - 10.5, x0 + CELL_W + 0.7, y + 10.5, 2.0, Color32::from_rgba_unmultiplied(120, 255, 120, glow));
            }
            let w = (CELL_W - 4.0) / hits as f32;
            for k in 0..hits {
                let sx = x0 + 2.0 + w * k as f32;
                fill(ui, g, sx + 0.6, lamp_y - 2.0, sx + w - 0.6, lamp_y + 2.0, 0.8, Color32::from_rgb(235, 255, 225));
            }
        }
        // ACCENT: red bar along the top of the pad
        if accent {
            fill(ui, g, x0 + 1.6, y - 9.0, x0 + CELL_W - 1.6, y - 6.0, 0.6, if outside { Color32::from_rgb(90, 24, 20) } else { RED_LED });
        }
        // thin step divider, like the light-strip bars of the hardware
        ui.painter().line_segment([g.p(x0 + CELL_W, y - 9.0), g.p(x0 + CELL_W, y + 9.0)], Stroke::new(g.l(0.8), Color32::from_rgb(70, 30, 12)));
        // end of this voice's loop (when shorter than 16)
        if s + 1 == len && len < NUM_STEPS {
            ui.painter().line_segment([g.p(x0 + CELL_W, y - 10.5), g.p(x0 + CELL_W, y + 10.5)], Stroke::new(g.l(1.6), ORANGE_HI));
        }
        if cur == s as i32 {
            fill(ui, g, x0 + 0.8, y - 9.0, x0 + CELL_W - 0.8, y + 9.0, 0.0, Color32::from_white_alpha(60));
        }
        let resp = ui.interact(cell, Id::new(("cell", ch, s)), g.sense(Sense::click_and_drag()));
        match mode {
            PAD_MULTI => {
                if resp.clicked() {
                    state.cycle_hits(prog, ch, s);
                }
            }
            PAD_ACCENT => {
                if resp.clicked() {
                    state.toggle_accent(prog, ch, s);
                }
            }
            PAD_PROB => {
                let r = if on { resp.on_hover_text(format!("Step {}: {} % chance", s + 1, chance)) } else { resp };
                if r.clicked() {
                    state.cycle_prob(prog, ch, s);
                }
            }
            PAD_LENGTH => {
                if resp.on_hover_text(format!("Voice {}: loop of {} steps", VOICE_LETTERS[ch], s + 1)).clicked() {
                    state.set_voice_length(prog, ch, s + 1);
                }
            }
            _ => {
                if resp.is_pointer_button_down_on() && st.paint.is_none() {
                    st.paint = Some(!on);
                    state.set_step(prog, ch, s, !on);
                }
            }
        }
    }
    // the voice's own length, on a small display at the end of the row
    let lcd = g.r(521.0, y - 6.0, 538.0, y + 6.0);
    let poly = len != state.length(prog);
    ui.painter().rect_filled(lcd, g.l(1.5), Color32::from_rgb(30, 4, 4));
    text(ui, g, 529.5, y, &format!("{len:02}"), 7.0, if poly { RED_LED } else { Color32::from_rgb(150, 40, 30) });
    // drag painting across cells (NOTE mode)
    if mode == PAD_NOTE {
        if let Some(v) = st.paint {
            if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                if bar.contains(pos) {
                    let rel = (pos.x - g.p(GRID_X0, 0.0).x) / g.l(CELL_W);
                    if rel >= 0.0 && rel < 16.0 {
                        state.set_step(prog, ch, rel as usize, v);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- right column

fn draw_right(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState) {
    let now = ui.input(|i| i.time);
    if st.chain_p > 0.0 {
        chain_area(ui, g, state, st, now);
    } else {
    // name plate
    let plate = g.r(590.0, 40.0, 728.0, 182.0);
    vgrad(ui, plate, Color32::from_rgb(240, 232, 212), Color32::from_rgb(214, 202, 176));
    ui.painter().rect_stroke(plate, g.l(3.0), Stroke::new(g.l(2.0), Color32::from_rgb(90, 62, 34)), egui::StrokeKind::Inside);
    text(ui, g, 659.0, 60.0, "GERANIUM", 10.0, Color32::from_rgb(60, 36, 20));
    text(ui, g, 659.0, 71.0, "RHYTHM  SYNTH", 6.5, Color32::from_rgb(90, 60, 36));
    text(ui, g, 659.0, 170.0, concat!("v", env!("CARGO_PKG_VERSION")), 6.5, Color32::from_rgb(90, 60, 36));
    text(ui, g, 659.0, 88.0, "RITMO · 6", 22.0, Color32::from_rgb(150, 24, 18));
    // factory kit selector (the original's 16 programs)
    let kit = state.kit.load(Relaxed) as usize;
    ui.painter().rect_filled(g.r(628.0, 108.0, 690.0, 128.0), g.l(2.0), Color32::from_rgb(30, 4, 4));
    let kit_name = if kit == 0 {
        "MINI".to_string()
    } else if kit == MP7_KIT_CHOICE {
        "MP7".to_string()
    } else {
        format!("KIT {:02}", kit)
    };
    text(ui, g, 659.0, 118.0, &kit_name, 11.0, RED_LED);
    let kl = ui.interact(g.r(606.0, 108.0, 626.0, 128.0), Id::new("kit_dn"), g.sense(Sense::click()));
    let kr = ui.interact(g.r(692.0, 108.0, 712.0, 128.0), Id::new("kit_up"), g.sense(Sense::click()));
    let arrow = |x: f32, dir: f32| vec![g.p(x - 5.0 * dir, 118.0), g.p(x + 5.0 * dir, 112.0), g.p(x + 5.0 * dir, 124.0)];
    ui.painter().add(Shape::convex_polygon(arrow(617.0, 1.0), Color32::from_rgb(60, 36, 20), Stroke::NONE));
    ui.painter().add(Shape::convex_polygon(arrow(701.0, -1.0).into_iter().rev().collect(), Color32::from_rgb(60, 36, 20), Stroke::NONE));
    let new_kit = if kl.on_hover_text("Previous factory kit").clicked() {
        Some((kit + NUM_KIT_CHOICES - 1) % NUM_KIT_CHOICES)
    } else if kr.on_hover_text("Next factory kit").clicked() {
        Some((kit + 1) % NUM_KIT_CHOICES)
    } else {
        None
    };
    if let Some(k) = new_kit {
        state.kit.store(k as u8, Relaxed);
        apply_kit(access, k);
        let msg = if k == 0 {
            "MINI kit loaded (Mini sound)".to_string()
        } else if k == MP7_KIT_CHOICE {
            "MP7 kit loaded (MP7 sound: bass drum, snare, conga, claves, hihat, quijada)".to_string()
        } else {
            format!("Factory kit {} loaded (Original sound)", k)
        };
        say(st, Some(msg), now);
    }
    let lamp = state.running.load(Relaxed) || state.host_driven.load(Relaxed);
    ui.painter().circle_filled(g.p(659.0, 146.0), g.l(3.5), if lamp { RED_LED } else { Color32::from_rgb(90, 24, 20) });
    if round_button(ui, g, Id::new("bank_load"), 626.0, 148.0, 8.0, None).on_hover_text("LOAD: open a bank file — the 16 rhythm programs (with MULTIPLY), all sounds, mixer and tempo").clicked() {
        start_dialog(st, now, || bank::load_bank_task());
    }
    if round_button(ui, g, Id::new("bank_save"), 692.0, 148.0, 8.0, None).on_hover_text("SAVE: save everything in one bank file — the 16 rhythm programs (with MULTIPLY), all sounds, mixer and tempo").clicked() {
        start_dialog(st, now, || bank::save_bank_task(access, state));
    }
    text(ui, g, 626.0, 168.0, "LOAD", 8.0, Color32::from_rgb(50, 30, 16));
    text(ui, g, 692.0, 168.0, "SAVE", 8.0, Color32::from_rgb(50, 30, 16));
    setup_button(ui, g, st);
    }

    // MIDI channel display
    let midi = g.r(590.0, 188.0, 728.0, 252.0);
    vgrad(ui, midi, Color32::from_rgb(40, 42, 40), Color32::from_rgb(18, 19, 18));
    let lcd = g.r(632.0, 196.0, 688.0, 226.0);
    ui.painter().rect_filled(lcd, g.l(2.0), Color32::from_rgb(30, 4, 4));
    let chn = state.midi_channel.load(Relaxed);
    let label = if chn == 0 { "All".to_string() } else { chn.to_string() };
    text(ui, g, 660.0, 211.0, &label, 15.0, RED_LED);
    let left = ui.interact(g.r(606.0, 200.0, 628.0, 222.0), Id::new("midi_dn"), g.sense(Sense::click()));
    let right = ui.interact(g.r(692.0, 200.0, 714.0, 222.0), Id::new("midi_up"), g.sense(Sense::click()));
    let tri = |x: f32, dir: f32| {
        vec![g.p(x - 5.0 * dir, 211.0), g.p(x + 5.0 * dir, 205.0), g.p(x + 5.0 * dir, 217.0)]
    };
    ui.painter().add(Shape::convex_polygon(tri(617.0, 1.0), INK, Stroke::NONE));
    ui.painter().add(Shape::convex_polygon(tri(703.0, -1.0).into_iter().rev().collect(), INK, Stroke::NONE));
    if left.clicked() {
        state.midi_channel.store((chn + 16) % 17, Relaxed);
    }
    if right.clicked() {
        state.midi_channel.store((chn + 1) % 17, Relaxed);
    }
    text(ui, g, 659.0, 240.0, "MIDI  CHANNEL", 8.0, INK);

    // module slots
        for ch in 0..NUM_CH {
        let x = 596.0 + ch as f32 * 21.5;
        let slot = g.r(x, 264.0, x + 18.0, 456.0);
        let resp = ui.interact(slot, Id::new(("slot", ch)), g.sense(Sense::click()));
        let hot = resp.hovered();
        vgrad(ui, slot, if hot { Color32::from_rgb(70, 74, 72) } else { Color32::from_rgb(44, 46, 46) }, Color32::from_rgb(18, 19, 19));
        ui.painter().rect_stroke(slot, g.l(1.0), Stroke::new(g.l(1.0), Color32::from_rgb(10, 10, 10)), egui::StrokeKind::Inside);
        let name = format!("{}0{}", VOICE_LETTERS[ch], ch + 1);
        text(ui, g, x + 9.0, 273.0, &name, 6.5, INK);
        // TUNE on every strip, with the note it plays (click the note to tune exactly to it)
        let tune_id = ParamId::Voice(ch, VoiceParam::ToneTune);
        let strip_label = if sound_engine(access) == SoundEngine::Mp7 { "FREQ" } else { "TUNE" };
        text(ui, g, x + 9.0, 283.0, strip_label, 4.5, Color32::from_rgb(230, 200, 140));
        knob(ui, g, access, tune_id, x + 9.0, 293.0, 6.5);
        let tune = access.get(tune_id);
        let (note, cents) = crate::params::note_of_volts(tune);
        let lcd = g.r(x + 1.0, 302.0, x + 17.0, 313.0);
        let nr = ui.interact(lcd, Id::new(("slot_note", ch)), g.sense(Sense::click()));
        ui.painter().rect_filled(lcd, g.l(1.5), if nr.hovered() { Color32::from_rgb(50, 8, 8) } else { Color32::from_rgb(30, 4, 4) });
        text(ui, g, x + 9.0, 307.5, &note, 5.5, RED_LED);
        if nr
            .on_hover_text(format!("{note} {cents:+} cents ({:.1} Hz). Click to tune exactly to {note}.", crate::params::volts_to_hz(tune)))
            .clicked()
        {
            access.set_gesture(tune_id, crate::params::snap_to_note(tune).clamp(0.0, 10.0));
            state.trig_req.fetch_or(1 << ch, Relaxed);
        }
        // the voice's four FX sends (post-fader, post-pan)
        for k in 0..NUM_FX_BUSES {
            let yy = 324.0 + k as f32 * 27.0;
            text(ui, g, x + 9.0, yy, FX_BUS_SHORT[k], 5.0, Color32::from_rgb(150, 200, 160));
            knob(ui, g, access, ParamId::Send(ch, k), x + 9.0, yy + 11.0, 6.5);
        }
        let lamp = now - st.lamp_time[ch] < 0.12;
        ui.painter().circle_filled(g.p(x + 9.0, 446.0), g.l(2.5), if lamp { RED_LED } else { Color32::from_rgb(70, 20, 18) });
        if resp.on_hover_text(format!("Open sound module {}", VOICE_LETTERS[ch])).clicked() {
            st.open_module = Some(ch);
        }
    }
}

/// The CHAIN button (above FX): turns the name plate over to show the pattern chain behind it.
/// Its lamp is lit while the chain is ON.
fn chain_button(ui: &mut egui::Ui, g: G, state: &SharedState, st: &mut UiState) {
    let r = g.r(604.0, 382.0, 650.0, 402.0);
    let resp = ui.interact(r, Id::new("chain_button"), g.sense(Sense::click()));
    fill(ui, g, 604.0, 384.0, 650.0, 404.0, 3.0, Color32::from_black_alpha(150));
    fill(ui, g, 604.0, 382.0, 650.0, 402.0, 3.0, Color32::from_rgb(20, 20, 22));
    let (top, bot) = if resp.is_pointer_button_down_on() {
        (Color32::from_rgb(170, 70, 30), Color32::from_rgb(200, 96, 44))
    } else if resp.hovered() {
        (Color32::from_rgb(250, 140, 70), Color32::from_rgb(200, 90, 40))
    } else {
        (Color32::from_rgb(232, 120, 56), Color32::from_rgb(180, 76, 30))
    };
    vgrad_d(ui, g, 607.0, 385.0, 647.0, 399.0, top, bot);
    text(ui, g, 620.0, 372.0, "CHAIN", 8.5, ORANGE_HI);
    let lit = state.chain_on.load(Relaxed);
    ui.painter().circle_filled(g.p(644.0, 372.0), g.l(3.0), if lit { RED_LED } else { Color32::from_rgb(70, 20, 18) });
    let tip = if st.chain_open { "Close the pattern chain" } else { "Pattern chain: play several programs one after another (lamp: chain ON)" };
    if resp.on_hover_text(tip).clicked() {
        st.chain_open = !st.chain_open;
        st.open_module = None;
    }
}

const PLATE_X0: f32 = 590.0;
const PLATE_Y0: f32 = 40.0;
const PLATE_X1: f32 = 728.0;
const PLATE_Y1: f32 = 182.0;
const PLATE_TOP: Color32 = Color32::from_rgb(240, 232, 212);
const PLATE_BOT: Color32 = Color32::from_rgb(214, 202, 176);
const PLATE_INK: Color32 = Color32::from_rgb(60, 36, 20);

/// The name plate's place: the plate itself, the chain panel on its back, or the plate turning
/// over around its middle (0.65 s, like the FX panel).
fn chain_area(ui: &mut egui::Ui, g: G, state: &SharedState, st: &mut UiState, now: f64) {
    if st.chain_p >= 1.0 {
        chain_panel(ui, g, state, st, now);
        return;
    }
    // the recess behind the turning plate
    fill(ui, g, PLATE_X0, PLATE_Y0, PLATE_X1, PLATE_Y1, 3.0, Color32::from_rgb(12, 12, 14));
    let th = flip_angle(st.chain_p, st.chain_open);
    let (c, sn) = (th.cos(), th.sin());
    let (hx, hy) = ((PLATE_X0 + PLATE_X1) / 2.0, (PLATE_Y0 + PLATE_Y1) / 2.0);
    if c >= 0.0 {
        plate_front_face(ui, g.tilted_about(c, sn, hx, hy));
    } else {
        chain_panel(ui, g.tilted_about(-c, -sn, hx, hy), state, st, now);
    }
}

/// The name plate as seen while it turns (its look; the controls work once it is flat).
fn plate_front_face(ui: &egui::Ui, g: G) {
    vgrad_d(ui, g, PLATE_X0, PLATE_Y0, PLATE_X1, PLATE_Y1, PLATE_TOP, PLATE_BOT);
    text(ui, g, 659.0, 60.0, "GERANIUM", 10.0, PLATE_INK);
    text(ui, g, 659.0, 71.0, "RHYTHM  SYNTH", 6.5, Color32::from_rgb(90, 60, 36));
    text(ui, g, 659.0, 88.0, "RITMO · 6", 22.0, Color32::from_rgb(150, 24, 18));
    fill(ui, g, 628.0, 108.0, 690.0, 128.0, 2.0, Color32::from_rgb(30, 4, 4));
    for x in [626.0, 692.0] {
        ui.painter().circle_filled(g.p(x, 148.0), g.l(8.0), Color32::from_rgb(28, 30, 32));
    }
}

/// Chain slots: 4 rows of 4, read left to right like the mock-up.
const CHAIN_X0: f32 = 600.0;
const CHAIN_DX: f32 = 32.0;
const CHAIN_W: f32 = 23.0;
const CHAIN_Y0: f32 = 49.0;
const CHAIN_DY: f32 = 22.0;
const CHAIN_H: f32 = 13.0;

fn chain_slot_rect(i: usize) -> (f32, f32) {
    (CHAIN_X0 + (i % 4) as f32 * CHAIN_DX, CHAIN_Y0 + (i / 4) as f32 * CHAIN_DY)
}

/// The back of the name plate: 16 chain slots joined by the orange line, ON / OFF, RANDOM and
/// TAKE YOUR PICK. Drawn the same flat or turning (only flat can be clicked).
fn chain_panel(ui: &mut egui::Ui, g: G, state: &SharedState, st: &mut UiState, now: f64) {
    let orange = Color32::from_rgb(236, 132, 58);
    vgrad_d(ui, g, PLATE_X0, PLATE_Y0, PLATE_X1, PLATE_Y1, PLATE_TOP, PLATE_BOT);
    let edge = Stroke::new(g.l(2.0), Color32::from_rgb(90, 62, 34));
    let corners = [g.p(PLATE_X0, PLATE_Y0), g.p(PLATE_X1, PLATE_Y0), g.p(PLATE_X1, PLATE_Y1), g.p(PLATE_X0, PLATE_Y1)];
    for k in 0..4 {
        ui.painter().line_segment([corners[k], corners[(k + 1) % 4]], edge);
    }
    // the line that runs through the chain, row after row
    let line = Stroke::new(g.l(1.3), orange);
    let (xl, xr) = (CHAIN_X0 - 5.0, CHAIN_X0 + 3.0 * CHAIN_DX + CHAIN_W + 5.0);
    for r in 0..4 {
        let yc = CHAIN_Y0 + r as f32 * CHAIN_DY + CHAIN_H / 2.0;
        ui.painter().line_segment([g.p(xl, yc), g.p(xr, yc)], line);
        if r < 3 {
            let gap = CHAIN_Y0 + r as f32 * CHAIN_DY + CHAIN_H + (CHAIN_DY - CHAIN_H) / 2.0;
            let yn = yc + CHAIN_DY;
            ui.painter().add(Shape::line(vec![g.p(xr, yc), g.p(xr, gap), g.p(xl, gap), g.p(xl, yn)], line));
        }
    }
    let len = state.chain_len();
    let playing = state.chain_slot.load(Relaxed);
    let on = state.chain_on.load(Relaxed);
    for i in 0..NUM_CHAIN {
        let (x, y) = chain_slot_rect(i);
        let resp = ui.interact(g.r(x, y, x + CHAIN_W, y + CHAIN_H), Id::new(("chain_slot", i)), g.sense(Sense::click()));
        let here = on && playing == i as i8;
        if here {
            fill(ui, g, x - 1.5, y - 1.5, x + CHAIN_W + 1.5, y + CHAIN_H + 1.5, 2.5, ORANGE_HI);
        } else if resp.hovered() {
            fill(ui, g, x - 1.0, y - 1.0, x + CHAIN_W + 1.0, y + CHAIN_H + 1.0, 2.0, Color32::from_rgb(150, 110, 80));
        }
        fill(ui, g, x, y, x + CHAIN_W, y + CHAIN_H, 1.5, Color32::from_rgb(18, 18, 20));
        let prog = state.chain_program(i);
        if let Some(p) = prog {
            // slots after the first empty one aren't played: dimmed
            let col = if i < len { RED_LED } else { Color32::from_rgb(110, 34, 28) };
            text(ui, g, x + CHAIN_W / 2.0, y + CHAIN_H / 2.0, &format!("{:02}", p + 1), 8.0, col);
        }
        if !g.live {
            continue;
        }
        let cur = state.chain[i].load(Relaxed);
        let mut new = None;
        if resp.clicked() {
            new = Some((cur + 1) % 17);
        } else if resp.secondary_clicked() {
            new = Some(0);
        } else if resp.hovered() {
            let d = ui.input(|i| i.raw_scroll_delta.y);
            if d > 0.0 {
                new = Some((cur + 1) % 17);
            } else if d < 0.0 {
                new = Some((cur + 16) % 17);
            }
        }
        let tip = match prog {
            Some(p) => format!("Slot {}: program {:02}. Click or wheel: another program · right-click: empty.\nThe chain plays from slot 1 up to the first empty slot, then starts again.", i + 1, p + 1),
            None => format!("Slot {}: empty. Click or wheel: choose a program.\nThe chain plays from slot 1 up to the first empty slot, then starts again.", i + 1),
        };
        resp.on_hover_text(tip);
        if let Some(v) = new {
            state.chain[i].store(v, Relaxed);
        }
    }
    // buttons
    let random = state.chain_random.load(Relaxed);
    let buttons: [(f32, f32, &str, Option<bool>); 3] =
        [(596.0, 626.0, "ON / OFF", Some(on)), (630.0, 662.0, "RANDOM", Some(random)), (666.0, 722.0, "TAKE YOUR PICK", None)];
    let mut pressed = [false; 3];
    for (k, (x0, x1, name, lit)) in buttons.iter().enumerate() {
        let (y0, y1) = (139.0, 154.0);
        let resp = ui.interact(g.r(*x0, y0, *x1, y1), Id::new(("chain_btn", k)), g.sense(Sense::click()));
        let lit = lit.unwrap_or(false);
        let (top, bot) = if lit {
            (Color32::from_rgb(255, 176, 96), Color32::from_rgb(244, 132, 60))
        } else if resp.hovered() {
            (Color32::from_rgb(246, 140, 76), Color32::from_rgb(214, 100, 46))
        } else {
            (Color32::from_rgb(232, 120, 58), Color32::from_rgb(196, 88, 38))
        };
        fill(ui, g, *x0, y0 + 1.0, *x1, y1 + 1.0, 2.0, Color32::from_black_alpha(90));
        vgrad_d(ui, g, *x0, y0, *x1, y1, top, bot);
        text(ui, g, (x0 + x1) / 2.0, (y0 + y1) / 2.0, name, 5.6, Color32::from_rgb(34, 18, 10));
        if k < 2 {
            ui.painter().circle_filled(g.p((x0 + x1) / 2.0, y1 + 5.5), g.l(1.8), if lit { RED_LED } else { Color32::from_rgb(120, 60, 50) });
        }
        let tip = match k {
            0 => "ON / OFF: play the chain (the lamp is lit while it is on)",
            1 => "RANDOM: play the chained patterns in random order (never the same twice in a row)",
            _ => "TAKE YOUR PICK: RITMO-6 fills the chain with 2 to 16 patterns at random, with no gap",
        };
        if resp.on_hover_text(tip).clicked() {
            pressed[k] = true;
        }
    }
    if pressed[0] {
        let v = !on;
        state.chain_on.store(v, Relaxed);
        let msg = if !v {
            "Chain off".to_string()
        } else if len == 0 {
            "Chain on: put a program in slot 1 to start it".to_string()
        } else {
            format!("Chain on: {len} pattern{}", if len > 1 { "s" } else { "" })
        };
        say(st, Some(msg), now);
    }
    if pressed[1] {
        state.chain_random.store(!random, Relaxed);
        say(st, Some(if random { "Chain in order" } else { "Chain in random order" }.into()), now);
    }
    if pressed[2] {
        let n = take_your_pick(state, st);
        say(st, Some(format!("Take your pick: {n} patterns in the chain")), now);
    }
    text(ui, g, 659.0, 170.0, "PATTERN  CHAIN", 6.5, PLATE_INK);
}

/// TAKE YOUR PICK: 2 to 16 slots from the first one, each a program drawn at random among the
/// programs that have notes (any program if none has), never twice the same side by side when
/// there is a choice; the slots after them are emptied, so there is never a gap.
fn take_your_pick(state: &SharedState, st: &mut UiState) -> usize {
    let n = 2 + (st.rand15() % 15) as usize;
    let mut pool: Vec<u8> = (0..16).filter(|&p| !state.program_empty(p)).map(|p| p as u8 + 1).collect();
    if pool.is_empty() {
        pool = (1..=16).collect();
    }
    let mut prev = 0u8;
    for i in 0..NUM_CHAIN {
        let v = if i < n {
            let mut v = pool[st.rand15() as usize % pool.len()];
            while pool.len() > 1 && v == prev {
                v = pool[st.rand15() as usize % pool.len()];
            }
            v
        } else {
            0
        };
        state.chain[i].store(v, Relaxed);
        prev = v;
    }
    n
}

/// The small gear at the top right of the name plate: opens the SETUP window.
fn setup_button(ui: &mut egui::Ui, g: G, st: &mut UiState) {
    let (cx, cy) = (716.0, 52.0);
    let resp = ui.interact(g.r(cx - 8.0, cy - 8.0, cx + 8.0, cy + 13.0), Id::new("setup_button"), g.sense(Sense::click()));
    let col = if st.setup.open || resp.hovered() { Color32::from_rgb(150, 24, 18) } else { Color32::from_rgb(70, 44, 24) };
    let p = ui.painter();
    let c = g.p(cx, cy);
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        p.circle_filled(c + vec2(a.cos(), a.sin()) * g.l(4.6), g.l(1.5), col);
    }
    p.circle_filled(c, g.l(4.2), col);
    p.circle_filled(c, g.l(1.7), Color32::from_rgb(232, 222, 198));
    text(ui, g, cx, cy + 9.5, "SETUP", 4.5, col);
    if resp.on_hover_text("SETUP: window size; audio output and MIDI inputs (standalone app)").clicked() {
        st.setup.toggle();
    }
}

// ---------------------------------------------------------------- sound module editor

fn draw_module(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState, ch: usize) {
    let now = ui.input(|i| i.time);
    let frame = g.r(586.0, 36.0, 732.0, 466.0);
    vgrad(ui, frame, Color32::from_rgb(236, 120, 36), Color32::from_rgb(196, 84, 18));
    let board = g.r(594.0, 46.0, 724.0, 458.0);
    vgrad(ui, board, Color32::from_rgb(44, 45, 48), Color32::from_rgb(26, 27, 30));
    // faint traces
    for k in 0..9 {
        let y = 70.0 + k as f32 * 44.0;
        ui.painter().line_segment([g.p(598.0, y), g.p(720.0, y + 12.0)], Stroke::new(g.l(1.0), Color32::from_rgb(64, 76, 66)));
    }
    let is_mp7 = sound_engine(access) == SoundEngine::Mp7;
    let inst = mp7::instrument_index(access.get(ParamId::Mp7Inst(ch)));
    let title = if is_mp7 {
        format!("MODULE - {}0{} · MP7", VOICE_LETTERS[ch], ch + 1)
    } else {
        format!("MODULE - {}0{}", VOICE_LETTERS[ch], ch + 1)
    };
    ui.painter().rect_filled(g.r(600.0, 50.0, 718.0, 66.0), g.l(2.0), Color32::from_rgb(20, 22, 20));
    text(ui, g, 659.0, 58.0, &title, 9.5, INK);

    let vk = |p: VoiceParam| ParamId::Voice(ch, p);
    let gold = Color32::from_rgb(230, 200, 140);
    if is_mp7 {
        draw_mp7_module(ui, g, access, state, st, ch, inst, now);
    } else {
    // TONE OSC: sine with pitch envelope and phase modulation
    let tone: [(VoiceParam, f32, f32); 7] = [
        (VoiceParam::ToneTune, 614.0, 90.0),
        (VoiceParam::ToneDecay, 659.0, 90.0),
        (VoiceParam::ToneVolume, 704.0, 90.0),
        (VoiceParam::PitchDecay, 614.0, 128.0),
        (VoiceParam::PitchDecayVol, 659.0, 128.0),
        (VoiceParam::PhaseModRate, 614.0, 166.0),
        (VoiceParam::PhaseModLev, 659.0, 166.0),
    ];
    for (p, x, y) in tone {
        knob(ui, g, access, vk(p), x, y, 10.0);
        text(ui, g, x, y + 16.0, voice_spec(p).short, 6.0, INK);
    }
    // the note TUNE sits on (to match a kick to the key of a song); click = snap to that note
    let tune = access.get(vk(VoiceParam::ToneTune));
    let (note, cents) = crate::params::note_of_volts(tune);
    let lcd = g.r(684.0, 118.0, 724.0, 136.0);
    let resp = ui.interact(lcd, Id::new(("tune_note", ch)), g.sense(Sense::click()));
    ui.painter().rect_filled(lcd, g.l(2.0), if resp.hovered() { Color32::from_rgb(50, 8, 8) } else { Color32::from_rgb(30, 4, 4) });
    text(ui, g, 704.0, 124.5, &note, 9.0, RED_LED);
    let off = if cents == 0 { "in tune".to_string() } else { format!("{cents:+} ct") };
    text(ui, g, 704.0, 132.0, &off, 5.0, Color32::from_rgb(200, 60, 50));
    if resp
        .on_hover_text(format!("TUNE note: {note} {cents:+} cents ({:.1} Hz). Click to tune exactly to {note}.", crate::params::volts_to_hz(tune)))
        .clicked()
    {
        access.set_gesture(vk(VoiceParam::ToneTune), crate::params::snap_to_note(tune).clamp(0.0, 10.0));
        state.trig_req.fetch_or(1 << ch, Relaxed);
    }
    text(ui, g, 704.0, 147.0, "TONE", 8.0, gold);
    text(ui, g, 704.0, 158.0, "OSC", 8.0, gold);
    }

    // LOAD / RAND / SAVE
    let lb = g.r(598.0, 196.0, 634.0, 212.0);
    let sb = g.r(684.0, 196.0, 720.0, 212.0);
    ui.painter().rect_filled(lb, g.l(3.0), Color32::from_rgb(26, 44, 120));
    ui.painter().rect_filled(sb, g.l(3.0), Color32::from_rgb(26, 44, 120));
    text(ui, g, 616.0, 204.0, "LOAD", 7.5, INK);
    text(ui, g, 702.0, 204.0, "SAVE", 7.5, INK);
    if ui.interact(lb, Id::new(("mod_load", ch)), g.sense(Sense::click())).on_hover_text("Load a module sound").clicked() {
        start_dialog(st, now, || bank::load_module_task(ch));
    }
    if ui.interact(sb, Id::new(("mod_save", ch)), g.sense(Sense::click())).on_hover_text("Save this module sound").clicked() {
        start_dialog(st, now, || bank::save_module_task(access, ch));
    }
    if round_button(ui, g, Id::new(("mod_rand", ch)), 659.0, 204.0, 12.0, Some(Color32::from_rgb(196, 90, 70)))
        .on_hover_text(if is_mp7 { "Random variation of this MP7 instrument" } else { "Random sound for this module" })
        .clicked()
    {
        if is_mp7 {
            randomize_mp7_voice(access, st, ch, inst);
        } else {
            randomize_voice(access, st, ch);
        }
        state.trig_req.fetch_or(1 << ch, Relaxed);
        say(st, Some(format!("Module {} randomized", VOICE_LETTERS[ch])), now);
    }
    text(ui, g, 659.0, 204.0, "RAND", 6.5, Color32::WHITE);

    // NOISE OSC + FILTER (the MP7 module draws its own)
    if !is_mp7 {
    let noise: [(VoiceParam, f32, f32); 5] = [
        (VoiceParam::NoiseVolume, 614.0, 244.0),
        (VoiceParam::NoiseDecay, 659.0, 244.0),
        (VoiceParam::FilterDecay, 704.0, 244.0),
        (VoiceParam::FilterFreq, 614.0, 282.0),
        (VoiceParam::FilterReso, 659.0, 282.0),
    ];
    for (p, x, y) in noise {
        knob(ui, g, access, vk(p), x, y, 10.0);
        text(ui, g, x, y + 16.0, voice_spec(p).short, 6.0, INK);
    }
    let hp_v = access.get(vk(VoiceParam::FilterHp));
    let hp = hp_v >= FILTER_HP_THRESHOLD;
    if toggle_switch(ui, g, Id::new(("mod_hp", ch)), 704.0, 282.0, hp)
        .on_hover_text("Noise filter: low-pass / high-pass")
        .clicked()
    {
        // In one gesture, first the value the panel shows, then the new one: a host that had
        // fallen out of step with the plugin (Cubase VST3 after a module LOAD) is brought back
        // in step, and always sees a change, so the click can't be swallowed.
        let id = vk(VoiceParam::FilterHp);
        access.begin(id);
        access.set(id, if hp { 10.0 } else { 0.0 });
        access.set(id, if hp { 0.0 } else { 10.0 });
        access.end(id);
    }
    text(ui, g, 694.0, 296.0, "LP", 6.0, INK);
    text(ui, g, 714.0, 296.0, "HP", 6.0, INK);
    text(ui, g, 659.0, 316.0, "NOISE  OSC  ·  FILTER", 8.0, gold);
    }

    // PAN RANDOM (requested addition) + audition
    knob(ui, g, access, vk(VoiceParam::PanRandom), 626.0, 356.0, 11.0);
    text(ui, g, 626.0, 374.0, "PAN RND", 6.5, INK);
    let lamp = now - st.lamp_time[ch] < 0.12;
    if round_button(ui, g, Id::new(("mod_trig", ch)), 692.0, 356.0, 10.0, lamp.then_some(ORANGE_HI)).on_hover_text("Play").clicked() {
        state.trig_req.fetch_or(1 << ch, Relaxed);
    }
    text(ui, g, 692.0, 374.0, "TRIG", 6.5, INK);

    let close = g.r(612.0, 420.0, 706.0, 444.0);
    let r = ui.interact(close, Id::new("mod_close"), g.sense(Sense::click()));
    ui.painter().rect_filled(close, g.l(4.0), if r.hovered() { Color32::from_rgb(80, 84, 82) } else { Color32::from_rgb(50, 54, 52) });
    text(ui, g, 659.0, 432.0, "CLOSE", 9.0, INK);
    if r.clicked() {
        st.open_module = None;
    }
}

/// The sound module of a voice on the MP7 engine: its instrument (one of 15, with its preset
/// sound) and the 13 sound knobs under their MP7 names. Knobs an instrument doesn't use are
/// dimmed.
#[allow(clippy::too_many_arguments)]
fn draw_mp7_module(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState, ch: usize, inst: usize, now: f64) {
    let gold = Color32::from_rgb(230, 200, 140);
    let labels = mp7::knob_labels(inst);
    let slots: [(VoiceParam, f32, f32); 12] = [
        (VoiceParam::ToneTune, 614.0, 90.0),
        (VoiceParam::ToneDecay, 659.0, 90.0),
        (VoiceParam::ToneVolume, 704.0, 90.0),
        (VoiceParam::PitchDecay, 614.0, 128.0),
        (VoiceParam::PitchDecayVol, 659.0, 128.0),
        (VoiceParam::PhaseModRate, 614.0, 166.0),
        (VoiceParam::PhaseModLev, 659.0, 166.0),
        (VoiceParam::NoiseVolume, 614.0, 244.0),
        (VoiceParam::NoiseDecay, 659.0, 244.0),
        (VoiceParam::FilterDecay, 704.0, 244.0),
        (VoiceParam::FilterFreq, 614.0, 282.0),
        (VoiceParam::FilterReso, 659.0, 282.0),
    ];
    for (p, x, y) in slots {
        let slot = p.index();
        let label = labels[slot];
        let id = ParamId::Voice(ch, p);
        let hover = move |v: f32| match label {
            Some(l) => format!("{l}: {}", mp7::describe(inst, slot, v)),
            None => format!("Not used by the {}", mp7::INSTRUMENT_NAMES[inst]),
        };
        knob_with(ui, g, access, id, x, y, 10.0, Some(&hover));
        if label.is_none() {
            ui.painter().circle_filled(g.p(x, y), g.l(10.5), Color32::from_black_alpha(150));
        }
        text(ui, g, x, y + 16.0, label.unwrap_or("-"), 6.0, if label.is_some() { INK } else { INK_DIM });
    }

    // the note FREQ sits on (click = tune exactly to it)
    let tune_id = ParamId::Voice(ch, VoiceParam::ToneTune);
    let tune = access.get(tune_id);
    let (note, cents) = crate::params::note_of_volts(tune);
    let lcd = g.r(684.0, 118.0, 724.0, 136.0);
    let resp = ui.interact(lcd, Id::new(("tune_note", ch)), g.sense(Sense::click()));
    ui.painter().rect_filled(lcd, g.l(2.0), if resp.hovered() { Color32::from_rgb(50, 8, 8) } else { Color32::from_rgb(30, 4, 4) });
    text(ui, g, 704.0, 124.5, &note, 9.0, RED_LED);
    let off = if cents == 0 { "in tune".to_string() } else { format!("{cents:+} ct") };
    text(ui, g, 704.0, 132.0, &off, 5.0, Color32::from_rgb(200, 60, 50));
    if resp
        .on_hover_text(format!("FREQ note: {note} {cents:+} cents ({:.1} Hz). Click to tune exactly to {note}.", crate::params::volts_to_hz(tune)))
        .clicked()
    {
        access.set_gesture(tune_id, crate::params::snap_to_note(tune).clamp(0.0, 10.0));
        state.trig_req.fetch_or(1 << ch, Relaxed);
    }

    // INSTRUMENT selector: the LCD reloads the instrument's own sound, the arrows pick another
    let lcd = g.r(678.0, 141.0, 724.0, 157.0);
    let lr = ui.interact(lcd, Id::new(("mp7_inst_lcd", ch)), g.sense(Sense::click()));
    ui.painter().rect_filled(lcd, g.l(2.0), if lr.hovered() { Color32::from_rgb(50, 8, 8) } else { Color32::from_rgb(30, 4, 4) });
    text(ui, g, 701.0, 149.0, mp7::INSTRUMENT_SHORT[inst], 6.5, RED_LED);
    let left = ui.interact(g.r(679.0, 160.0, 695.0, 172.0), Id::new(("mp7_inst_dn", ch)), g.sense(Sense::click()));
    let right = ui.interact(g.r(707.0, 160.0, 723.0, 172.0), Id::new(("mp7_inst_up", ch)), g.sense(Sense::click()));
    let tri = |x: f32, dir: f32| vec![g.p(x - 4.0 * dir, 166.0), g.p(x + 4.0 * dir, 161.5), g.p(x + 4.0 * dir, 170.5)];
    let col = |r: &Response| if r.hovered() { ORANGE_HI } else { INK };
    ui.painter().add(Shape::convex_polygon(tri(687.0, 1.0), col(&left), Stroke::NONE));
    ui.painter().add(Shape::convex_polygon(tri(715.0, -1.0).into_iter().rev().collect(), col(&right), Stroke::NONE));
    text(ui, g, 701.0, 177.0, "INSTRUMENT", 4.8, gold);
    let n = mp7::NUM_INSTRUMENTS;
    let pick = if left.on_hover_text("Previous MP7 instrument").clicked() {
        Some((inst + n - 1) % n)
    } else if right.on_hover_text("Next MP7 instrument").clicked() {
        Some((inst + 1) % n)
    } else if lr
        .on_hover_text(format!("{}: click to bring back its own sound", mp7::INSTRUMENT_NAMES[inst]))
        .clicked()
    {
        Some(inst)
    } else {
        None
    };
    if let Some(i) = pick {
        load_mp7_instrument(access, ch, i);
        state.trig_req.fetch_or(1 << ch, Relaxed);
        say(st, Some(format!("Voice {}: MP7 {}", VOICE_LETTERS[ch], mp7::INSTRUMENT_NAMES[i])), now);
    }

    // DARK switch (the LP/HP switch's place)
    let dark_id = ParamId::Voice(ch, VoiceParam::FilterHp);
    let dark = access.get(dark_id) >= FILTER_HP_THRESHOLD;
    if toggle_switch(ui, g, Id::new(("mod_hp", ch)), 704.0, 282.0, dark)
        .on_hover_text("NORM / DARK: DARK rounds off the top, like a worn old box")
        .clicked()
    {
        access.begin(dark_id);
        access.set(dark_id, if dark { 10.0 } else { 0.0 });
        access.set(dark_id, if dark { 0.0 } else { 10.0 });
        access.end(dark_id);
    }
    text(ui, g, 692.0, 296.0, "NORM", 5.0, INK);
    text(ui, g, 716.0, 296.0, "DARK", 5.0, INK);
    text(ui, g, 659.0, 316.0, &format!("MP7  ·  {}", mp7::INSTRUMENT_NAMES[inst].to_uppercase()), 8.0, gold);
}

/// RAND on the MP7 engine: the instrument's own sound, each used knob moved a little at random
/// (up to about a sixth of its travel), so it stays recognisable.
fn randomize_mp7_voice(access: &dyn ParamAccess, st: &mut UiState, ch: usize, inst: usize) {
    let base = mp7::preset(inst);
    let labels = mp7::knob_labels(inst);
    for (i, p) in crate::params::VOICE_PARAMS.iter().enumerate().take(12) {
        if labels[i].is_none() {
            continue;
        }
        let r = st.rand15() as f32 / 32767.0 * 2.0 - 1.0;
        let v = (base[i] + r * 1.6).clamp(0.0, 9.99999);
        access.set_gesture(ParamId::Voice(ch, *p), v);
    }
}

/// RAND, as the original's module randomizer (OCET13 0x10001195): knobs in panel order get
/// rand() * 10 / 32767 volts; the 2nd, 4th and 9th are halved (0..5 V) and the 6th and 8th
/// (Phase Mod Rate, Tone Volume) are left alone. Pan Random (an addition) isn't touched.
fn randomize_voice(access: &dyn ParamAccess, st: &mut UiState, ch: usize) {
    const PANEL_ORDER: [VoiceParam; 13] = [
        VoiceParam::ToneDecay,
        VoiceParam::PitchDecay,
        VoiceParam::PitchDecayVol,
        VoiceParam::NoiseDecay,
        VoiceParam::FilterDecay,
        VoiceParam::PhaseModRate,
        VoiceParam::ToneTune,
        VoiceParam::ToneVolume,
        VoiceParam::PhaseModLev,
        VoiceParam::NoiseVolume,
        VoiceParam::FilterFreq,
        VoiceParam::FilterReso,
        VoiceParam::FilterHp,
    ];
    for (i, p) in PANEL_ORDER.iter().enumerate() {
        if i == 5 || i == 7 {
            continue;
        }
        let mut v = st.rand15() as f32 * 10.0 / 32767.0;
        if i == 1 || i == 3 || i == 8 {
            v *= 0.5;
        }
        access.set_gesture(ParamId::Voice(ch, *p), v.min(9.99999));
    }
}