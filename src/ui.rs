//! The front panel, drawn by hand so the plugin and the standalone app look identical.

use crate::bank;
use crate::params::{
    apply_kit, internal_bpm, spec, sync_division, voice_spec, ParamAccess, ParamId, VoiceParam,
    FILTER_HP_THRESHOLD, NUM_CH, NUM_KIT_CHOICES, NUM_STEPS, SYNC_NAMES, VOICE_LETTERS,
};
use crate::state::SharedState;
use egui::{pos2, vec2, Align2, Color32, FontId, Id, Pos2, Rect, Response, Sense, Shape, Stroke};
use std::sync::atomic::Ordering::Relaxed;

pub const W: f32 = 760.0;
pub const H: f32 = 490.0;

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
    clipboard: Option<([u16; NUM_CH], [u32; NUM_CH], u8)>,
    seen_hits: [u32; NUM_CH],
    lamp_time: [f64; NUM_CH],
    message: String,
    message_time: f64,
    rng: u64,
    chassis: Option<egui::TextureHandle>,
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

/// Maps design coordinates (760x490) to the screen.
#[derive(Clone, Copy)]
struct G {
    s: f32,
    o: Pos2,
}

impl G {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.o.x + x * self.s, self.o.y + y * self.s)
    }
    fn r(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(self.p(x0, y0), self.p(x1, y1))
    }
    fn f(&self, size: f32) -> FontId {
        FontId::proportional(size * self.s)
    }
    fn l(&self, w: f32) -> f32 {
        w * self.s
    }
}

pub fn draw(ctx: &egui::Context, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(BG))
        .show(ctx, |ui| {
            let avail = ui.max_rect();
            let s = (avail.width() / W).min(avail.height() / H).max(0.2);
            let o = avail.center() - vec2(W * s / 2.0, H * s / 2.0);
            let g = G { s, o };
            let now = ctx.input(|i| i.time);
            for ch in 0..NUM_CH {
                let h = state.hits[ch].load(Relaxed);
                if h != st.seen_hits[ch] {
                    st.seen_hits[ch] = h;
                    st.lamp_time[ch] = now;
                }
            }
            draw_case(ui, g, st);
            draw_main(ui, g, access, state, st, now);
            if let Some(ch) = st.open_module {
                draw_module(ui, g, access, state, st, ch);
            } else {
                draw_right(ui, g, access, state, st);
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
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(30));
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

fn text(ui: &egui::Ui, g: G, x: f32, y: f32, t: &str, size: f32, color: Color32) {
    ui.painter().text(g.p(x, y), Align2::CENTER_CENTER, t, g.f(size), color);
}

/// The 3D-rendered plastic chassis (tools/chassis.py, rendered with Blender at 2x).
const CHASSIS_PNG: &[u8] = include_bytes!("../assets/chassis.png");

fn chassis_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(CHASSIS_PNG).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
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
    let sp = spec(id);
    let rect = g.r(cx - r - 2.0, cy - r - 2.0, cx + r + 2.0, cy + r + 2.0);
    let resp = ui.interact(rect, Id::new(("knob", id)), Sense::click_and_drag());
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
    let shown = sp.format(access.get(id));
    resp.on_hover_text(format!("{}: {}", sp.name, shown))
}

fn round_button(ui: &mut egui::Ui, g: G, id: Id, cx: f32, cy: f32, r: f32, lit: Option<Color32>) -> Response {
    let rect = g.r(cx - r, cy - r, cx + r, cy + r);
    let resp = ui.interact(rect, id, Sense::click());
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

fn toggle_switch(ui: &mut egui::Ui, g: G, id: Id, cx: f32, cy: f32, on: bool) -> Response {
    let rect = g.r(cx - 14.0, cy - 6.0, cx + 14.0, cy + 6.0);
    let resp = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, g.l(6.0), Color32::from_rgb(14, 15, 16));
    let kx = if on { cx + 7.0 } else { cx - 7.0 };
    p.circle_filled(g.p(kx, cy), g.l(5.0), Color32::from_rgb(196, 200, 206));
    p.circle_filled(g.p(kx - 1.0, cy - 1.5), g.l(2.0), Color32::WHITE);
    resp
}

/// Horizontal fader with the level wedge above it.
fn hfader(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, id: ParamId, x0: f32, x1: f32, y: f32, wedge: Color32) {
    let sp = spec(id);
    let rect = g.r(x0 - 6.0, y - 18.0, x1 + 6.0, y + 8.0);
    let resp = ui.interact(rect, Id::new(("fader", id)), Sense::click_and_drag());
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
    p.rect_filled(g.r(x0, y - 1.5, x1, y + 1.5), g.l(1.0), Color32::from_rgb(8, 9, 10));
    // cap
    let cap = g.r(xn - 5.0, y - 7.0, xn + 5.0, y + 7.0);
    vgrad(ui, cap, Color32::from_rgb(235, 236, 238), Color32::from_rgb(120, 124, 130));
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
            .on_hover_text("Mute")
            .clicked()
        {
            access.set_gesture(ParamId::Mute(ch), if muted { 0.0 } else { 1.0 });
        }
        draw_row(ui, g, state, st, prog, ch, len, cur);
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

    // SOUND selector: MINI (first-day engine) or ORIGINAL (decoded model)
    let original = access.get(ParamId::Sound) >= 0.5;
    text(ui, g, 140.0, 321.0, if original { "SOUND: ORIG" } else { "SOUND: MINI" }, 7.5, ORANGE_HI);
    if toggle_switch(ui, g, Id::new("sound"), 140.0, 334.0, original)
        .on_hover_text("Sound engine: MINI = the punchy first-day sound, ORIGINAL = the model decoded from the plugin")
        .clicked()
    {
        access.set_gesture(ParamId::Sound, if original { 0.0 } else { 1.0 });
    }

    // MULTIPLY mode selector (requested addition)
    let mult = state.multiply_mode.load(Relaxed);
    text(ui, g, 196.0, 321.0, "MULTIPLY", 7.5, if mult { GREEN_HI } else { INK });
    if toggle_switch(ui, g, Id::new("multiply"), 196.0, 334.0, mult)
        .on_hover_text("MULTIPLY: each click on a pad gives 1, 2, 3, 4 hits inside the step, then off")
        .clicked()
    {
        state.multiply_mode.store(!mult, Relaxed);
    }
    ui.painter().circle_filled(g.p(216.0, 334.0), g.l(2.5), if mult { GREEN_HI } else { Color32::from_rgb(20, 50, 22) });

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
            .on_hover_text(format!("Pattern length: {} steps", i + 1))
            .clicked()
        {
            state.lengths[prog].store(i as u8 + 1, Relaxed);
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
                    for c in 0..NUM_CH {
                        state.steps[prog][c].store(0, Relaxed);
                        state.multi[prog][c].store(0, Relaxed);
                    }
                    state.lengths[prog].store(16, Relaxed);
                    say(st, Some(format!("Program {} cleared", prog + 1)), now);
                }
                1 => {
                    let rows = std::array::from_fn(|c| state.steps[prog][c].load(Relaxed));
                    let multi = std::array::from_fn(|c| state.multi[prog][c].load(Relaxed));
                    st.clipboard = Some((rows, multi, len as u8));
                    say(st, Some(format!("Program {} copied", prog + 1)), now);
                }
                2 => {
                    if let Some((rows, multi, l)) = st.clipboard {
                        for c in 0..NUM_CH {
                            state.steps[prog][c].store(rows[c], Relaxed);
                            state.multi[prog][c].store(multi[c], Relaxed);
                        }
                        state.lengths[prog].store(l, Relaxed);
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
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_row(ui: &mut egui::Ui, g: G, state: &SharedState, st: &mut UiState, prog: usize, ch: usize, len: usize, cur: i32) {
    let y = row_y(ch);
    let bar = g.r(GRID_X0 - 3.0, y - 10.0, GRID_X0 + CELL_W * 16.0 + 3.0, y + 10.0);
    ui.painter().rect_filled(bar.expand(g.l(1.0)), g.l(2.0), Color32::from_rgb(16, 12, 10));
    for s in 0..NUM_STEPS {
        let x0 = GRID_X0 + CELL_W * s as f32;
        let cell = g.r(x0 + 0.8, y - 9.0, x0 + CELL_W - 0.8, y + 9.0);
        let on = state.step_on(prog, ch, s);
        let hits = state.hits_at(prog, ch, s);
        // Subdivided steps light green: faint for 2 hits, brightest for 4.
        let green = match hits {
            2 => Some((Color32::from_rgb(70, 150, 70), Color32::from_rgb(28, 88, 30))),
            3 => Some((Color32::from_rgb(120, 210, 100), Color32::from_rgb(45, 145, 45))),
            4 => Some((Color32::from_rgb(200, 255, 170), GREEN_HI)),
            _ => None,
        };
        let (top, bot) = if s >= len {
            (ORANGE_DEAD, Color32::from_rgb(36, 22, 16))
        } else if let Some(gc) = green {
            gc
        } else if on {
            (Color32::from_rgb(255, 236, 170), ORANGE_HI)
        } else {
            (Color32::from_rgb(214, 104, 50), Color32::from_rgb(168, 70, 30))
        };
        vgrad(ui, cell, top, bot);
        if on && hits <= 1 {
            ui.painter().rect_filled(g.r(x0 + 3.0, y - 2.0, x0 + CELL_W - 3.0, y + 2.0), g.l(1.0), Color32::WHITE);
            ui.painter().rect_filled(cell.expand(g.l(1.5)), g.l(2.0), Color32::from_rgba_unmultiplied(255, 220, 120, 40));
        } else if hits >= 2 {
            // one small light per hit inside the step
            let glow = 20 + 25 * hits as u8;
            ui.painter().rect_filled(cell.expand(g.l(1.5)), g.l(2.0), Color32::from_rgba_unmultiplied(120, 255, 120, glow));
            let w = (CELL_W - 4.0) / hits as f32;
            for k in 0..hits {
                let sx = x0 + 2.0 + w * k as f32;
                ui.painter().rect_filled(g.r(sx + 0.6, y - 2.0, sx + w - 0.6, y + 2.0), g.l(0.8), Color32::from_rgb(235, 255, 225));
            }
        }
        // thin step divider, like the light-strip bars of the hardware
        ui.painter().line_segment([g.p(x0 + CELL_W, y - 9.0), g.p(x0 + CELL_W, y + 9.0)], Stroke::new(g.l(0.8), Color32::from_rgb(70, 30, 12)));
        if cur == s as i32 {
            ui.painter().rect_filled(cell, 0.0, Color32::from_white_alpha(60));
        }
        let resp = ui.interact(cell, Id::new(("cell", ch, s)), Sense::click_and_drag());
        if state.multiply_mode.load(Relaxed) {
            if resp.clicked() {
                state.cycle_hits(prog, ch, s);
            }
        } else if resp.is_pointer_button_down_on() && st.paint.is_none() {
            st.paint = Some(!on);
            state.set_step(prog, ch, s, !on);
        }
    }
    // drag painting across cells
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

// ---------------------------------------------------------------- right column

fn draw_right(ui: &mut egui::Ui, g: G, access: &dyn ParamAccess, state: &SharedState, st: &mut UiState) {
    let now = ui.input(|i| i.time);
    // name plate
    let plate = g.r(590.0, 40.0, 728.0, 182.0);
    vgrad(ui, plate, Color32::from_rgb(240, 232, 212), Color32::from_rgb(214, 202, 176));
    ui.painter().rect_stroke(plate, g.l(3.0), Stroke::new(g.l(2.0), Color32::from_rgb(90, 62, 34)), egui::StrokeKind::Inside);
    text(ui, g, 659.0, 60.0, "GERANIUM", 10.0, Color32::from_rgb(60, 36, 20));
    text(ui, g, 659.0, 71.0, "RHYTHM  SYNTH", 6.5, Color32::from_rgb(90, 60, 36));
    text(ui, g, 659.0, 88.0, "RITMO · 6", 22.0, Color32::from_rgb(150, 24, 18));
    // factory kit selector (the original's 16 programs)
    let kit = state.kit.load(Relaxed) as usize;
    ui.painter().rect_filled(g.r(628.0, 108.0, 690.0, 128.0), g.l(2.0), Color32::from_rgb(30, 4, 4));
    text(ui, g, 659.0, 118.0, &(if kit == 0 { "MINI".to_string() } else { format!("KIT {:02}", kit) }), 11.0, RED_LED);
    let kl = ui.interact(g.r(606.0, 108.0, 626.0, 128.0), Id::new("kit_dn"), Sense::click());
    let kr = ui.interact(g.r(692.0, 108.0, 712.0, 128.0), Id::new("kit_up"), Sense::click());
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
        say(st, Some(if k == 0 { "MINI kit loaded (Mini sound)".into() } else { format!("Factory kit {} loaded (Original sound)", k) }), now);
    }
    let lamp = state.running.load(Relaxed) || state.host_driven.load(Relaxed);
    ui.painter().circle_filled(g.p(659.0, 146.0), g.l(3.5), if lamp { RED_LED } else { Color32::from_rgb(90, 24, 20) });
    if round_button(ui, g, Id::new("bank_load"), 626.0, 148.0, 8.0, None).on_hover_text("Load a bank (all programs and modules)").clicked() {
        let m = bank::load_bank_dialog(access, state);
        say(st, m, now);
    }
    if round_button(ui, g, Id::new("bank_save"), 692.0, 148.0, 8.0, None).on_hover_text("Save the bank").clicked() {
        let m = bank::save_bank_dialog(access, state);
        say(st, m, now);
    }
    text(ui, g, 626.0, 168.0, "LOAD", 8.0, Color32::from_rgb(50, 30, 16));
    text(ui, g, 692.0, 168.0, "SAVE", 8.0, Color32::from_rgb(50, 30, 16));

    // MIDI channel display
    let midi = g.r(590.0, 188.0, 728.0, 252.0);
    vgrad(ui, midi, Color32::from_rgb(40, 42, 40), Color32::from_rgb(18, 19, 18));
    let lcd = g.r(632.0, 196.0, 688.0, 226.0);
    ui.painter().rect_filled(lcd, g.l(2.0), Color32::from_rgb(30, 4, 4));
    let chn = state.midi_channel.load(Relaxed);
    let label = if chn == 0 { "All".to_string() } else { chn.to_string() };
    text(ui, g, 660.0, 211.0, &label, 15.0, RED_LED);
    let left = ui.interact(g.r(606.0, 200.0, 628.0, 222.0), Id::new("midi_dn"), Sense::click());
    let right = ui.interact(g.r(692.0, 200.0, 714.0, 222.0), Id::new("midi_up"), Sense::click());
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
        let resp = ui.interact(slot, Id::new(("slot", ch)), Sense::click());
        let hot = resp.hovered();
        vgrad(ui, slot, if hot { Color32::from_rgb(70, 74, 72) } else { Color32::from_rgb(44, 46, 46) }, Color32::from_rgb(18, 19, 19));
        ui.painter().rect_stroke(slot, g.l(1.0), Stroke::new(g.l(1.0), Color32::from_rgb(10, 10, 10)), egui::StrokeKind::Inside);
        let name = format!("{}0{}", VOICE_LETTERS[ch], ch + 1);
        for (k, c) in name.chars().enumerate() {
            text(ui, g, x + 9.0, 278.0 + k as f32 * 12.0, &c.to_string(), 9.5, INK);
        }
        // little circuit details
        for k in 0..6 {
            let yy = 330.0 + k as f32 * 18.0;
            ui.painter().rect_filled(g.r(x + 4.0, yy, x + 14.0, yy + 6.0), 0.0, Color32::from_rgb(60, 92, 70));
        }
        let lamp = now - st.lamp_time[ch] < 0.12;
        ui.painter().circle_filled(g.p(x + 9.0, 446.0), g.l(2.5), if lamp { RED_LED } else { Color32::from_rgb(70, 20, 18) });
        if resp.on_hover_text(format!("Open sound module {}", VOICE_LETTERS[ch])).clicked() {
            st.open_module = Some(ch);
        }
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
    let title = format!("MODULE - {}0{}", VOICE_LETTERS[ch], ch + 1);
    ui.painter().rect_filled(g.r(600.0, 50.0, 718.0, 66.0), g.l(2.0), Color32::from_rgb(20, 22, 20));
    text(ui, g, 659.0, 58.0, &title, 9.5, INK);

    let vk = |p: VoiceParam| ParamId::Voice(ch, p);
    let gold = Color32::from_rgb(230, 200, 140);
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
    text(ui, g, 704.0, 147.0, "TONE", 8.0, gold);
    text(ui, g, 704.0, 158.0, "OSC", 8.0, gold);

    // LOAD / RAND / SAVE
    let lb = g.r(598.0, 196.0, 634.0, 212.0);
    let sb = g.r(684.0, 196.0, 720.0, 212.0);
    ui.painter().rect_filled(lb, g.l(3.0), Color32::from_rgb(26, 44, 120));
    ui.painter().rect_filled(sb, g.l(3.0), Color32::from_rgb(26, 44, 120));
    text(ui, g, 616.0, 204.0, "LOAD", 7.5, INK);
    text(ui, g, 702.0, 204.0, "SAVE", 7.5, INK);
    if ui.interact(lb, Id::new(("mod_load", ch)), Sense::click()).on_hover_text("Load a module sound").clicked() {
        let m = bank::load_module_dialog(access, ch);
        say(st, m, now);
    }
    if ui.interact(sb, Id::new(("mod_save", ch)), Sense::click()).on_hover_text("Save this module sound").clicked() {
        let m = bank::save_module_dialog(access, ch);
        say(st, m, now);
    }
    if round_button(ui, g, Id::new(("mod_rand", ch)), 659.0, 204.0, 12.0, Some(Color32::from_rgb(196, 90, 70)))
        .on_hover_text("Random sound for this module")
        .clicked()
    {
        randomize_voice(access, st, ch);
        state.trig_req.fetch_or(1 << ch, Relaxed);
        say(st, Some(format!("Module {} randomized", VOICE_LETTERS[ch])), now);
    }
    text(ui, g, 659.0, 204.0, "RAND", 6.5, Color32::WHITE);

    // NOISE OSC + FILTER
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
        access.set_gesture(vk(VoiceParam::FilterHp), if hp { 0.0 } else { 10.0 });
    }
    text(ui, g, 694.0, 296.0, "LP", 6.0, INK);
    text(ui, g, 714.0, 296.0, "HP", 6.0, INK);
    text(ui, g, 659.0, 316.0, "NOISE  OSC  ·  FILTER", 8.0, gold);

    // PAN RANDOM (requested addition) + audition
    knob(ui, g, access, vk(VoiceParam::PanRandom), 626.0, 356.0, 11.0);
    text(ui, g, 626.0, 374.0, "PAN RND", 6.5, INK);
    let lamp = now - st.lamp_time[ch] < 0.12;
    if round_button(ui, g, Id::new(("mod_trig", ch)), 692.0, 356.0, 10.0, lamp.then_some(ORANGE_HI)).on_hover_text("Play").clicked() {
        state.trig_req.fetch_or(1 << ch, Relaxed);
    }
    text(ui, g, 692.0, 374.0, "TRIG", 6.5, INK);

    let close = g.r(612.0, 420.0, 706.0, 444.0);
    let r = ui.interact(close, Id::new("mod_close"), Sense::click());
    ui.painter().rect_filled(close, g.l(4.0), if r.hovered() { Color32::from_rgb(80, 84, 82) } else { Color32::from_rgb(50, 54, 52) });
    text(ui, g, 659.0, 432.0, "CLOSE", 9.0, INK);
    if r.clicked() {
        st.open_module = None;
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