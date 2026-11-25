//! The SETUP window (opened by the small gear on the name plate), laid out like the options
//! window of big sampler apps: tabs on the left, settings on the right, Close at the bottom.
//!
//! - Interface: the window size, one of five fixed sizes. A plugin asks its host once for the
//!   new size (Cubase and some CLAP hosts handle a continuous drag badly).
//! - Audio and MIDI: only in the standalone app (in a plugin the host owns the devices).

use egui::{pos2, vec2, Align, Color32, FontId, Id, Layout, Rect, RichText, Sense, Stroke, UiBuilder};

/// Window sizes offered, in % of the design size (840 x 490 at 100 %).
pub const SIZES: [u32; 5] = [60, 80, 100, 130, 160];

/// Where the panel lives: a plugin window inside a host, or the standalone app's own window.
pub trait SetupHost {
    /// The panel's size now, as a share of the design size (1.0 = 840 x 490).
    fn scale(&self, ctx: &egui::Context) -> f32;
    /// Ask for one of the fixed sizes (1.0 = 100 %).
    fn set_scale(&mut self, ctx: &egui::Context, scale: f32);
    /// Audio and MIDI devices, when this program owns them (standalone app only).
    fn devices(&self) -> Option<DeviceView> {
        None
    }
    /// A change made on the Audio or MIDI page.
    fn change(&mut self, _change: DeviceChange) {}
}

/// A plugin, or anything that only has a window size.
pub struct NoDevices;

/// What the Audio and MIDI pages show (a snapshot; the lists are refreshed on demand).
#[derive(Clone, Debug, Default)]
pub struct DeviceView {
    pub drivers: Vec<String>,
    pub driver: String,
    pub outputs: Vec<String>,
    /// The device chosen (or the system default's name).
    pub output: String,
    /// Sample rates the device offers.
    pub sample_rates: Vec<u32>,
    /// None = the device's own rate.
    pub chosen_rate: Option<u32>,
    /// Buffer sizes (frames) the device accepts; empty = only its default.
    pub buffer_sizes: Vec<u32>,
    /// None = the device's default.
    pub chosen_buffer: Option<u32>,
    /// Stereo output pairs of the device (1 = outputs 1-2 only).
    pub pairs: usize,
    pub pair: usize,
    /// Running, or why not.
    pub status: String,
    pub running: bool,
    /// What the stream really runs at.
    pub running_rate: u32,
    /// Frames per audio block, as measured in the audio callback (0 = not known yet).
    pub block_frames: u32,
    /// MIDI inputs and whether each is on.
    pub midi_inputs: Vec<(String, bool)>,
}

#[derive(Clone, Debug)]
pub enum DeviceChange {
    Driver(String),
    Output(String),
    SampleRate(Option<u32>),
    Buffer(Option<u32>),
    Pair(usize),
    Midi(String, bool),
    /// Look for devices again (plugged in or out).
    Refresh,
}

impl SetupHost for NoDevices {
    fn scale(&self, ctx: &egui::Context) -> f32 {
        let r = ctx.screen_rect();
        (r.width() / crate::ui::W).min(r.height() / crate::ui::H)
    }
    fn set_scale(&mut self, _ctx: &egui::Context, _scale: f32) {}
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Interface,
    Audio,
    Midi,
}

#[derive(Default)]
pub struct SetupUi {
    pub open: bool,
    pub page: Page,
    /// The window was just opened: refresh the device lists once.
    fresh: bool,
}

impl SetupUi {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.fresh = self.open;
    }
    /// Open the window on one page (also used for pictures: `--setup audio`).
    pub fn show(&mut self, page: Page) {
        self.open = true;
        self.fresh = true;
        self.page = page;
    }
}

/// The size (%) closest to a scale, if it is one of SIZES (within 2 %).
pub fn size_of_scale(scale: f32) -> Option<u32> {
    SIZES.iter().copied().find(|&p| (scale * 100.0 - p as f32).abs() < 2.0)
}

const DIM: Color32 = Color32::from_black_alpha(175);
const FRAME: Color32 = Color32::from_rgb(58, 58, 58);
const TITLE: Color32 = Color32::from_rgb(222, 222, 222);
const BODY: Color32 = Color32::from_rgb(44, 44, 44);
const TABS: Color32 = Color32::from_rgb(52, 52, 52);
const TAB_ON: Color32 = Color32::from_rgb(78, 78, 78);
const INK: Color32 = Color32::from_rgb(214, 214, 214);
const INK_DIM: Color32 = Color32::from_rgb(150, 150, 150);
const FIELD: Color32 = Color32::from_rgb(28, 28, 28);

/// Draw the SETUP window over the panel (panel = its rectangle on screen, s = pixels per
/// design unit). Everything under it is dimmed and can't be clicked while it is open.
pub fn draw(ctx: &egui::Context, panel: Rect, s: f32, st: &mut SetupUi, host: &mut dyn SetupHost) {
    if !st.open {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.open = false;
        return;
    }
    let devices_owned = host.devices().is_some();
    if st.fresh {
        st.fresh = false;
        if devices_owned {
            host.change(DeviceChange::Refresh);
        }
    }
    if !devices_owned {
        st.page = Page::Interface;
    }
    let view = host.devices();

    egui::Area::new(Id::new("ritmo6-setup"))
        .order(egui::Order::Foreground)
        .fixed_pos(panel.min)
        .show(ctx, |ui| {
            // a cover over the whole panel: dims it and takes every click outside the window
            let (cover, _) = ui.allocate_exact_size(panel.size(), Sense::click_and_drag());
            ui.painter().rect_filled(cover, 0.0, DIM);

            let k = (s / 1.6).clamp(0.4, 2.5);
            let size = vec2(560.0, 340.0) * s;
            let win = Rect::from_center_size(cover.center(), size);
            let p = ui.painter().clone();
            p.rect_filled(win.expand(2.0 * k), 4.0 * k, Color32::from_black_alpha(160));
            p.rect_filled(win, 3.0 * k, FRAME);
            // title bar
            let title_h = 24.0 * s;
            let title = Rect::from_min_max(win.min, pos2(win.max.x, win.min.y + title_h));
            p.rect_filled(title, egui::CornerRadius { nw: (3.0 * k) as u8, ne: (3.0 * k) as u8, sw: 0, se: 0 }, TITLE);
            p.text(
                pos2(title.min.x + 10.0 * s, title.center().y),
                egui::Align2::LEFT_CENTER,
                "Settings",
                FontId::proportional(11.0 * s),
                Color32::from_rgb(30, 30, 30),
            );
            // tab list on the left, the page on the right, Close at the bottom
            let pad = 6.0 * s;
            let bottom_h = 26.0 * s;
            let tabs = Rect::from_min_max(pos2(win.min.x + pad, title.max.y + pad), pos2(win.min.x + pad + 96.0 * s, win.max.y - bottom_h));
            let page = Rect::from_min_max(pos2(tabs.max.x + pad, tabs.min.y), pos2(win.max.x - pad, win.max.y - bottom_h));
            p.rect_filled(tabs, 0.0, TABS);
            p.rect_filled(page, 0.0, BODY);
            p.rect_stroke(page, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(30, 30, 30)), egui::StrokeKind::Inside);

            let pages: &[(Page, &str)] = if devices_owned {
                &[(Page::Interface, "Interface"), (Page::Audio, "Audio"), (Page::Midi, "MIDI")]
            } else {
                &[(Page::Interface, "Interface")]
            };
            for (i, (pg, name)) in pages.iter().enumerate() {
                let r = Rect::from_min_size(pos2(tabs.min.x + 2.0 * s, tabs.min.y + 2.0 * s + i as f32 * 17.0 * s), vec2(tabs.width() - 4.0 * s, 15.0 * s));
                let resp = ui.interact(r, Id::new(("setup-tab", i)), Sense::click());
                if st.page == *pg {
                    p.rect_filled(r, 0.0, TAB_ON);
                } else if resp.hovered() {
                    p.rect_filled(r, 0.0, Color32::from_rgb(64, 64, 64));
                }
                p.text(pos2(r.min.x + 5.0 * s, r.center().y), egui::Align2::LEFT_CENTER, *name, FontId::proportional(9.0 * s), if st.page == *pg { Color32::WHITE } else { INK });
                if resp.clicked() {
                    st.page = *pg;
                }
            }

            // Close
            let close = Rect::from_min_size(pos2(win.max.x - pad - 70.0 * s, win.max.y - bottom_h + 5.0 * s), vec2(70.0 * s, 16.0 * s));
            let cr = ui.interact(close, Id::new("setup-close"), Sense::click());
            p.rect_filled(close, 2.0 * k, if cr.hovered() { Color32::from_rgb(110, 110, 110) } else { Color32::from_rgb(88, 88, 88) });
            p.text(close.center(), egui::Align2::CENTER_CENTER, "Close", FontId::proportional(9.0 * s), INK);
            if cr.clicked() {
                st.open = false;
            }

            let inner = page.shrink(10.0 * s);
            let mut pui = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
            scale_style(&mut pui, s);
            pui.set_clip_rect(page);
            match st.page {
                Page::Interface => interface_page(&mut pui, s, host, devices_owned),
                Page::Audio => {
                    if let Some(v) = &view {
                        audio_page(&mut pui, s, v, host);
                    }
                }
                Page::Midi => {
                    if let Some(v) = &view {
                        midi_page(&mut pui, s, v, host);
                    }
                }
            }
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

/// Fonts and spacing follow the panel's size, so the window looks the same at 60 % and 160 %.
fn scale_style(ui: &mut egui::Ui, s: f32) {
    let style = ui.style_mut();
    for (ts, font) in style.text_styles.iter_mut() {
        font.size = match ts {
            egui::TextStyle::Small => 6.5,
            egui::TextStyle::Heading => 11.0,
            _ => 8.5,
        } * s;
    }
    let sp = &mut style.spacing;
    sp.item_spacing = vec2(6.0, 5.0) * s;
    sp.button_padding = vec2(5.0, 2.0) * s;
    sp.interact_size = vec2(30.0, 14.0) * s;
    sp.icon_width = 9.0 * s;
    sp.icon_width_inner = 5.0 * s;
    sp.icon_spacing = 3.0 * s;
    sp.combo_width = 170.0 * s;
    let v = &mut style.visuals;
    v.override_text_color = Some(INK);
    v.widgets.inactive.weak_bg_fill = FIELD;
    v.widgets.inactive.bg_fill = FIELD;
    v.extreme_bg_color = FIELD;
}

fn label(ui: &mut egui::Ui, s: f32, t: &str) {
    ui.allocate_ui_with_layout(vec2(120.0 * s, 14.0 * s), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(120.0 * s);
        ui.label(RichText::new(t).color(INK));
    });
}

fn note(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).color(INK_DIM).small());
}

fn interface_page(ui: &mut egui::Ui, s: f32, host: &mut dyn SetupHost, standalone: bool) {
    ui.heading("Window size");
    ui.add_space(2.0 * s);
    let now = host.scale(ui.ctx());
    let current = size_of_scale(now);
    ui.horizontal(|ui| {
        for p in SIZES {
            let on = current == Some(p);
            let b = egui::Button::new(RichText::new(format!("{p} %")).color(if on { Color32::BLACK } else { INK }))
                .fill(if on { Color32::from_rgb(236, 132, 58) } else { FIELD })
                .min_size(vec2(44.0 * s, 18.0 * s));
            let tip = format!("{} x {} pixels", (crate::ui::W * p as f32 / 100.0).round(), (crate::ui::H * p as f32 / 100.0).round());
            if ui.add(b).on_hover_text(tip).clicked() && !on {
                host.set_scale(ui.ctx(), p as f32 / 100.0);
            }
        }
    });
    if current.is_none() {
        note(ui, &format!("Now: {:.0} % (a size set by hand).", now * 100.0));
    }
    ui.add_space(6.0 * s);
    if standalone {
        note(ui, "You can still drag the window's edges; the panel always fits the window.");
        note(ui, "This size, the audio device and the MIDI inputs are kept for the next start.");
    } else {
        note(ui, "The host is asked once for the new size. Your host keeps it with the project.");
        note(ui, "If the host refuses, the panel keeps its size.");
        ui.add_space(6.0 * s);
        note(ui, "Audio and MIDI devices are chosen in your host (Cubase, Bitwig, Live...).");
    }
}

fn audio_page(ui: &mut egui::Ui, s: f32, v: &DeviceView, host: &mut dyn SetupHost) {
    let mut change = None;
    egui::Grid::new("setup-audio").num_columns(2).spacing(vec2(8.0 * s, 6.0 * s)).show(ui, |ui| {
        label(ui, s, "Driver");
        egui::ComboBox::from_id_salt("setup-driver").selected_text(&v.driver).show_ui(ui, |ui| {
            for d in &v.drivers {
                if ui.selectable_label(*d == v.driver, d).clicked() && *d != v.driver {
                    change = Some(DeviceChange::Driver(d.clone()));
                }
            }
        });
        ui.end_row();

        label(ui, s, "Device");
        egui::ComboBox::from_id_salt("setup-output").selected_text(&v.output).width(220.0 * s).show_ui(ui, |ui| {
            for d in &v.outputs {
                if ui.selectable_label(*d == v.output, d).clicked() && *d != v.output {
                    change = Some(DeviceChange::Output(d.clone()));
                }
            }
        });
        ui.end_row();

        label(ui, s, "Outputs");
        let pair_name = |i: usize| format!("{} - {}", i * 2 + 1, i * 2 + 2);
        egui::ComboBox::from_id_salt("setup-pair").selected_text(pair_name(v.pair)).show_ui(ui, |ui| {
            for i in 0..v.pairs.max(1) {
                if ui.selectable_label(i == v.pair, pair_name(i)).clicked() && i != v.pair {
                    change = Some(DeviceChange::Pair(i));
                }
            }
        });
        ui.end_row();

        label(ui, s, "Sample rate");
        let rate_name = |r: Option<u32>| match r {
            Some(r) => format!("{r} Hz"),
            None => "Device default".to_string(),
        };
        egui::ComboBox::from_id_salt("setup-rate").selected_text(rate_name(v.chosen_rate)).show_ui(ui, |ui| {
            if ui.selectable_label(v.chosen_rate.is_none(), rate_name(None)).clicked() && v.chosen_rate.is_some() {
                change = Some(DeviceChange::SampleRate(None));
            }
            for &r in &v.sample_rates {
                if ui.selectable_label(v.chosen_rate == Some(r), rate_name(Some(r))).clicked() && v.chosen_rate != Some(r) {
                    change = Some(DeviceChange::SampleRate(Some(r)));
                }
            }
        });
        ui.end_row();

        label(ui, s, "Buffer size");
        let buf_name = |b: Option<u32>| match b {
            Some(b) => format!("{b} samples"),
            None => "Device default".to_string(),
        };
        ui.add_enabled_ui(!v.buffer_sizes.is_empty(), |ui| {
            egui::ComboBox::from_id_salt("setup-buffer").selected_text(buf_name(v.chosen_buffer)).show_ui(ui, |ui| {
                if ui.selectable_label(v.chosen_buffer.is_none(), buf_name(None)).clicked() && v.chosen_buffer.is_some() {
                    change = Some(DeviceChange::Buffer(None));
                }
                for &b in &v.buffer_sizes {
                    if ui.selectable_label(v.chosen_buffer == Some(b), buf_name(Some(b))).clicked() && v.chosen_buffer != Some(b) {
                        change = Some(DeviceChange::Buffer(Some(b)));
                    }
                }
            });
        });
        ui.end_row();

        label(ui, s, "Status");
        // long messages (a driver's error) wrap inside the page
        ui.scope(|ui| {
            ui.set_max_width(250.0 * s);
            ui.add(
                egui::Label::new(RichText::new(&v.status).color(if v.running { Color32::from_rgb(120, 220, 110) } else { Color32::from_rgb(255, 120, 90) }))
                    .wrap(),
            );
        });
        ui.end_row();

        label(ui, s, "Latency");
        if v.running && v.running_rate > 0 && v.block_frames > 0 {
            ui.label(format!(
                "{} samples = {:.1} ms at {} Hz",
                v.block_frames,
                v.block_frames as f32 * 1000.0 / v.running_rate as f32,
                v.running_rate
            ));
        } else if v.running && v.running_rate > 0 {
            ui.label(format!("measuring... ({} Hz)", v.running_rate));
        } else {
            ui.label("-");
        }
        ui.end_row();
    });
    ui.add_space(8.0 * s);
    ui.horizontal(|ui| {
        if ui.button("Look for devices again").clicked() {
            change = Some(DeviceChange::Refresh);
        }
        if v.driver == "ASIO" {
            note(ui, "ASIO: the lowest latency, one program at a time. Your card's own panel may set its buffer.");
        } else {
            note(ui, "Windows audio (WASAPI), shared with other programs.");
        }
    });
    if let Some(c) = change {
        host.change(c);
    }
}

fn midi_page(ui: &mut egui::Ui, s: f32, v: &DeviceView, host: &mut dyn SetupHost) {
    let mut change = None;
    ui.heading("Inputs");
    ui.add_space(2.0 * s);
    let w = ui.available_width();
    let row_h = 15.0 * s;
    let head = ui.allocate_exact_size(vec2(w, row_h), Sense::hover()).0;
    ui.painter().rect_filled(head, 0.0, Color32::from_rgb(64, 64, 64));
    ui.painter().text(pos2(head.min.x + 5.0 * s, head.center().y), egui::Align2::LEFT_CENTER, "Device name", FontId::proportional(8.0 * s), INK);
    ui.painter().text(pos2(head.max.x - 70.0 * s, head.center().y), egui::Align2::LEFT_CENTER, "Status", FontId::proportional(8.0 * s), INK);
    egui::ScrollArea::vertical().max_height(150.0 * s).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if v.midi_inputs.is_empty() {
            note(ui, "No MIDI input found.");
        }
        for (i, (name, on)) in v.midi_inputs.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(w, row_h), Sense::click());
            let bg = if resp.hovered() {
                Color32::from_rgb(60, 60, 60)
            } else if i % 2 == 0 {
                FIELD
            } else {
                Color32::from_rgb(34, 34, 34)
            };
            ui.painter().rect_filled(r, 0.0, bg);
            ui.painter().text(pos2(r.min.x + 5.0 * s, r.center().y), egui::Align2::LEFT_CENTER, name, FontId::proportional(8.0 * s), INK);
            ui.painter().text(
                pos2(r.max.x - 70.0 * s, r.center().y),
                egui::Align2::LEFT_CENTER,
                if *on { "On" } else { "Off" },
                FontId::proportional(8.0 * s),
                if *on { Color32::from_rgb(120, 220, 110) } else { INK_DIM },
            );
            if resp.on_hover_text("Click to switch this input on or off").clicked() {
                change = Some(DeviceChange::Midi(name.clone(), !on));
            }
        }
    });
    ui.add_space(6.0 * s);
    ui.horizontal(|ui| {
        if ui.button("Look for devices again").clicked() {
            change = Some(DeviceChange::Refresh);
        }
    });
    note(ui, "Notes 36-41 play voices A-F, 42-57 choose programs 1-16, 58 and up: hold to play.");
    note(ui, "The MIDI CHANNEL display on the panel filters the channel.");
    if let Some(c) = change {
        host.change(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_sizes_are_recognised() {
        assert_eq!(SIZES, [60, 80, 100, 130, 160]);
        for p in SIZES {
            assert_eq!(size_of_scale(p as f32 / 100.0), Some(p));
        }
        // a plugin window sized by hand with the old corner (1.0 - 2.0): no fixed size lit
        assert_eq!(size_of_scale(1.18), None);
        // the plugin rounds 840 x 0.6 etc. to whole pixels: still recognised
        assert_eq!(size_of_scale((crate::ui::W * 0.6).round() / crate::ui::W), Some(60));
    }
}
