//! Ritmo-6 standalone: the same panel and sound engine as the VST3, playing through the
//! default audio output, with MIDI input from the first MIDI port found.

use ritmo6_core::engine::{Engine, Transport, Values};
use ritmo6_core::params::{spec, ParamAccess, ParamId, NUM_PARAMS};
use ritmo6_core::state::SharedState;
use ritmo6_core::ui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::mpsc;
use std::sync::Arc;

struct Store {
    vals: Vec<AtomicU32>,
}

impl Store {
    fn new() -> Self {
        let mut vals: Vec<AtomicU32> = (0..NUM_PARAMS).map(|_| AtomicU32::new(0)).collect();
        for id in ParamId::all() {
            vals[id.index()] = AtomicU32::new(spec(id).default.to_bits());
        }
        Store { vals }
    }
}

impl ParamAccess for Store {
    fn get(&self, id: ParamId) -> f32 {
        f32::from_bits(self.vals[id.index()].load(Relaxed))
    }
    fn begin(&self, _id: ParamId) {}
    fn set(&self, id: ParamId, plain: f32) {
        let s = spec(id);
        self.vals[id.index()].store(plain.clamp(s.min, s.max).to_bits(), Relaxed);
    }
    fn end(&self, _id: ParamId) {}
}

enum Midi {
    On(u8, u8, f32),
    Off(u8, u8),
}

struct Audio {
    _stream: cpal::Stream,
    _midi: Option<midir::MidiInputConnection<()>>,
}

fn start_audio(store: Arc<Store>, state: Arc<SharedState>) -> Result<Audio, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let dev = host.default_output_device().ok_or("no audio output device")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    if cfg.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("unsupported sample format {:?}", cfg.sample_format()));
    }
    let channels = cfg.channels() as usize;
    let sr = cfg.sample_rate().0 as f32;
    let config: cpal::StreamConfig = cfg.into();

    let (tx, rx) = mpsc::channel::<Midi>();
    let midi = open_midi(tx);

    let mut engine = Engine::new(sr);
    let stream = dev
        .build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                let vals = Values::read(|id| store.get(id));
                while let Ok(m) = rx.try_recv() {
                    match m {
                        Midi::On(c, n, v) => engine.note_on(&state, c, n, v),
                        Midi::Off(c, n) => engine.note_off(&state, c, n),
                    }
                }
                engine.begin_block(&state, &vals, &Transport::default());
                for frame in data.chunks_mut(channels.max(1)) {
                    let f = engine.next_frame(&state, &vals);
                    for (i, x) in frame.iter_mut().enumerate() {
                        *x = match i {
                            0 => f.left,
                            1 => f.right,
                            _ => 0.0,
                        };
                    }
                }
            },
            |e| eprintln!("audio error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(Audio { _stream: stream, _midi: midi })
}

fn open_midi(tx: mpsc::Sender<Midi>) -> Option<midir::MidiInputConnection<()>> {
    let input = midir::MidiInput::new("Ritmo-6").ok()?;
    let ports = input.ports();
    let port = ports.first()?.clone();
    input
        .connect(
            &port,
            "ritmo6-in",
            move |_, msg, _| {
                if msg.len() < 3 {
                    return;
                }
                let kind = msg[0] & 0xF0;
                let ch = msg[0] & 0x0F;
                match kind {
                    0x90 if msg[2] > 0 => {
                        let _ = tx.send(Midi::On(ch, msg[1], msg[2] as f32 / 127.0));
                    }
                    0x80 | 0x90 => {
                        let _ = tx.send(Midi::Off(ch, msg[1]));
                    }
                    _ => {}
                }
            },
            (),
        )
        .ok()
}

struct App {
    store: Arc<Store>,
    state: Arc<SharedState>,
    ui: ui::UiState,
    shot: Option<PathBuf>,
    frames: u32,
    requested: bool,
    _audio: Option<Audio>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            let r = self.state.running.load(Relaxed);
            self.state.running.store(!r, Relaxed);
        }
        ui::draw(ctx, self.store.as_ref(), &self.state, &mut self.ui);

        if let Some(path) = self.shot.clone() {
            self.frames += 1;
            ctx.request_repaint();
            let image = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = image {
                let [w, h] = img.size;
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    Some(buf) => {
                        if let Err(e) = buf.save(&path) {
                            eprintln!("could not save screenshot: {e}");
                        }
                    }
                    None => eprintln!("screenshot had an unexpected size"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.frames >= 120 && !self.requested {
                self.requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let shot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    // --module <A..F> opens that sound module's panel at start (handy for pictures).
    // --size WxH opens the window at another size (the panel scales), e.g. for sharper pictures.
    let size = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .unwrap_or([ui::W, ui::H]);

    let open_module = args
        .iter()
        .position(|a| a == "--module")
        .and_then(|i| args.get(i + 1))
        .and_then(|m| ritmo6_core::params::VOICE_LETTERS.iter().position(|l| l.eq_ignore_ascii_case(m)));

    let store = Arc::new(Store::new());
    let state = Arc::new(SharedState::default());
    let audio = if shot.is_none() {
        match start_audio(store.clone(), state.clone()) {
            Ok(a) => Some(a),
            Err(e) => {
                eprintln!("no audio: {e}");
                None
            }
        }
    } else {
        None
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Ritmo-6 — rhythm synth")
        .with_inner_size(size)
        .with_min_inner_size([ui::W * 0.6, ui::H * 0.6]);
    if shot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if shot.is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }

    eframe::run_native(
        "Ritmo-6",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(App {
                store,
                state,
                ui: {
                    let mut u = ui::UiState::default();
                    u.open_module = open_module;
                    u
                },
                shot,
                frames: 0,
                requested: false,
                _audio: audio,
            }))
        }),
    )
}
