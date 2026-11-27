// A windowed program on Windows: no black console window opens behind the panel.
#![windows_subsystem = "windows"]

//! Ritmo-6 standalone: the same panel and sound engine as the VST3. The audio output and the
//! MIDI inputs are chosen in the SETUP window (the system default output and the first MIDI
//! input until then) and kept for the next start.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ritmo6_core::engine::{Engine, Transport, Values};
use ritmo6_core::params::{spec, ParamAccess, ParamId, NUM_PARAMS};
use ritmo6_core::setup::{DeviceChange, DeviceView, Page, SetupHost};
use ritmo6_core::state::SharedState;
use ritmo6_core::ui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

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

/// What the SETUP window chose, kept for the next start (in %APPDATA%\Ritmo-6\settings.json).
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
struct Settings {
    /// Audio driver (cpal host name: "WASAPI" or "ASIO" on Windows); None = the system's.
    driver: Option<String>,
    /// Output device name; None = the system default.
    output: Option<String>,
    sample_rate: Option<u32>,
    buffer: Option<u32>,
    /// Stereo output pair the Mix goes to (0 = outputs 1-2).
    pair: usize,
    /// MIDI inputs switched on; None = the first one found (as before).
    midi_inputs: Option<Vec<String>>,
    /// Window size (1.0 = 100 %).
    scale: Option<f32>,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("Ritmo-6").join("settings.json"))
    }
    fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
    fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, t);
        }
    }
}

/// Sample rates and buffer sizes offered when the device accepts them.
const RATES: [u32; 6] = [44100, 48000, 88200, 96000, 176400, 192000];
const BUFFERS: [u32; 7] = [32, 64, 128, 256, 512, 1024, 2048];

/// The standalone app's audio output and MIDI inputs, chosen in the SETUP window.
struct Devices {
    store: Arc<Store>,
    state: Arc<SharedState>,
    settings: Settings,
    /// False for pictures (--screenshot): no sound, no MIDI.
    live: bool,
    /// The audio driver in use. Kept open: ASIO allows one driver at a time per program.
    host: cpal::Host,
    midi_tx: mpsc::Sender<Midi>,
    /// The audio callback takes MIDI from here (try_lock: it never waits).
    midi_rx: Arc<Mutex<mpsc::Receiver<Midi>>>,
    stream: Option<cpal::Stream>,
    status: String,
    /// Set by the stream's error callback (device unplugged...).
    failed: Arc<Mutex<Option<String>>>,
    running_rate: u32,
    channels: u16,
    /// Frames in the last audio block (the real latency of one block).
    block: Arc<AtomicU32>,
    /// When the stream started: a device that asks for no sound 2 s later isn't really playing
    /// (an ASIO driver whose card is unplugged opens all the same).
    started: Option<std::time::Instant>,
    drivers: Vec<cpal::HostId>,
    outputs: Vec<String>,
    output_name: String,
    rates: Vec<u32>,
    buffers: Vec<u32>,
    midi_ports: Vec<String>,
    midi_conns: Vec<(String, midir::MidiInputConnection<()>)>,
}

impl Devices {
    fn new(store: Arc<Store>, state: Arc<SharedState>, settings: Settings, live: bool) -> Self {
        let (tx, rx) = mpsc::channel::<Midi>();
        let host = open_host(settings.driver.as_deref());
        let mut d = Devices {
            store,
            state,
            settings,
            live,
            host,
            midi_tx: tx,
            midi_rx: Arc::new(Mutex::new(rx)),
            stream: None,
            status: String::new(),
            failed: Arc::new(Mutex::new(None)),
            running_rate: 0,
            channels: 2,
            block: Arc::new(AtomicU32::new(0)),
            started: None,
            drivers: Vec::new(),
            outputs: Vec::new(),
            output_name: String::new(),
            rates: Vec::new(),
            buffers: Vec::new(),
            midi_ports: Vec::new(),
            midi_conns: Vec::new(),
        };
        d.scan();
        d.start();
        d.sync_midi();
        d
    }

    fn host_id(&self) -> cpal::HostId {
        self.host.id()
    }

    /// Look for audio drivers, output devices and MIDI inputs (again).
    fn scan(&mut self) {
        self.drivers = cpal::available_hosts();
        self.outputs = match asio_driver_names(&self.host) {
            // ASIO: read the installed drivers without loading them (loading another one
            // would stop the one playing)
            Some(names) => names,
            None => self
                .host
                .output_devices()
                .map(|ds| ds.filter_map(|d| d.name().ok()).collect())
                .unwrap_or_default(),
        };
        self.midi_ports = midir::MidiInput::new("Ritmo-6 scan")
            .map(|m| m.ports().iter().filter_map(|p| m.port_name(p).ok()).collect())
            .unwrap_or_default();
    }

    /// (Re)start the audio stream with the chosen device and settings. With no device chosen
    /// (or the chosen one gone), the default one plays, or else the first one that starts:
    /// an ASIO driver whose card isn't plugged in is skipped.
    fn start(&mut self) {
        self.stream = None; // the old stream stops before the new one opens
        self.running_rate = 0;
        self.block.store(0, Relaxed);
        if let Ok(mut f) = self.failed.lock() {
            *f = None;
        }
        let find = |host: &cpal::Host, name: &str| {
            host.output_devices().ok().and_then(|mut ds| ds.find(|d| d.name().ok().as_deref() == Some(name)))
        };
        let mut notes = Vec::new();
        if let Some(name) = self.settings.output.clone() {
            if let Some(dev) = find(&self.host, &name) {
                self.start_on(dev, notes);
                return;
            }
            notes.push(format!("{name} not found"));
        }
        let mut names: Vec<String> = Vec::new();
        if let Some(n) = self.host.default_output_device().and_then(|d| d.name().ok()) {
            names.push(n);
        }
        for n in &self.outputs {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
        let mut first_failure: Option<(String, String)> = None;
        for (i, name) in names.iter().enumerate() {
            let Some(dev) = find(&self.host, name) else { continue };
            let mut n = notes.clone();
            if i > 0 {
                n.push(format!("{} didn't start", names[0]));
            }
            if self.start_on(dev, n) {
                return;
            }
            first_failure.get_or_insert((self.output_name.clone(), self.status.clone()));
            if !self.live {
                break;
            }
        }
        match first_failure {
            Some((name, status)) => {
                self.output_name = name;
                self.status = status;
            }
            None => {
                self.output_name = "(none)".into();
                self.status = "No audio output found".into();
            }
        }
    }

    /// Start playing on one device; false (with the reason in status) when it can't.
    fn start_on(&mut self, dev: cpal::Device, mut notes: Vec<String>) -> bool {
        self.output_name = dev.name().unwrap_or_else(|_| "Unknown device".into());
        let default = match dev.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("This device can't play: {e}");
                return false;
            }
        };
        let channels = default.channels();
        self.channels = channels;
        // the sample format: the device's own when Ritmo-6 can write it (ASIO cards often
        // want 32- or 16-bit whole numbers), else 32-bit float
        let format = if FORMATS.contains(&default.sample_format()) { default.sample_format() } else { cpal::SampleFormat::F32 };
        let ranges: Vec<cpal::SupportedStreamConfigRange> = dev
            .supported_output_configs()
            .map(|it| it.filter(|r| r.sample_format() == format && r.channels() == channels).collect())
            .unwrap_or_default();
        let def_rate = default.sample_rate().0;
        let def_f32 = default.sample_format() == format;
        let fits = |r: u32| ranges.iter().any(|g| g.min_sample_rate().0 <= r && r <= g.max_sample_rate().0);
        let mut rates: Vec<u32> = RATES.iter().copied().filter(|&r| fits(r)).collect();
        if def_f32 && !rates.contains(&def_rate) {
            rates.push(def_rate);
            rates.sort_unstable();
        }
        self.rates = rates;
        self.buffers = match default.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => {
                BUFFERS.iter().copied().filter(|b| min <= b && b <= max).collect()
            }
            cpal::SupportedBufferSize::Unknown => Vec::new(),
        };
        if self.rates.is_empty() {
            self.status = format!("Unsupported sample format {:?}", default.sample_format());
            return false;
        }
        let rate = self.settings.sample_rate.filter(|r| self.rates.contains(r)).unwrap_or(def_rate);
        let rate = if self.rates.contains(&rate) { rate } else { self.rates[0] };
        let fixed = self.settings.buffer.filter(|b| self.buffers.contains(b));
        let mut config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(rate),
            buffer_size: fixed.map(cpal::BufferSize::Fixed).unwrap_or(cpal::BufferSize::Default),
        };
        let mut built = self.build_any(&dev, &config, format);
        if built.is_err() && fixed.is_some() {
            config.buffer_size = cpal::BufferSize::Default;
            notes.push("buffer size refused, using the device's".to_string());
            built = self.build_any(&dev, &config, format);
        }
        if built.is_err() && rate != def_rate && def_f32 {
            config.sample_rate = cpal::SampleRate(def_rate);
            notes.push("sample rate refused, using the device's".to_string());
            built = self.build_any(&dev, &config, format);
        }
        match built {
            Ok(stream) => {
                self.stream = Some(stream);
                self.started = Some(std::time::Instant::now());
                self.running_rate = config.sample_rate.0;
                let fmt = match format {
                    cpal::SampleFormat::I16 => ", 16-bit",
                    cpal::SampleFormat::I32 => ", 32-bit",
                    _ => "",
                };
                self.status = if notes.is_empty() { format!("Running{fmt}") } else { format!("Running{fmt} ({})", notes.join("; ")) };
                true
            }
            Err(e) => {
                self.status = format!("Can't start: {e}");
                false
            }
        }
    }

    fn build_any(&self, dev: &cpal::Device, config: &cpal::StreamConfig, format: cpal::SampleFormat) -> Result<cpal::Stream, String> {
        match format {
            cpal::SampleFormat::I32 => self.build::<i32>(dev, config),
            cpal::SampleFormat::I16 => self.build::<i16>(dev, config),
            _ => self.build::<f32>(dev, config),
        }
    }

    fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(&self, dev: &cpal::Device, config: &cpal::StreamConfig) -> Result<cpal::Stream, String> {
        if !self.live {
            return Err("sound is off while a picture is taken".into());
        }
        let store = self.store.clone();
        let state = self.state.clone();
        let rx = self.midi_rx.clone();
        let block = self.block.clone();
        let failed = self.failed.clone();
        let ch = (config.channels as usize).max(1);
        let pair = self.settings.pair.min((ch / 2).max(1) - 1);
        let (left, right) = (pair * 2, pair * 2 + 1);
        let mut engine = Engine::new(config.sample_rate.0 as f32);
        let stream = dev
            .build_output_stream(
                config,
                move |data: &mut [T], _| {
                    let vals = Values::read(|id| store.get(id));
                    if let Ok(rx) = rx.try_lock() {
                        while let Ok(m) = rx.try_recv() {
                            match m {
                                Midi::On(c, n, v) => engine.note_on(&state, c, n, v),
                                Midi::Off(c, n) => engine.note_off(&state, c, n),
                            }
                        }
                    }
                    engine.begin_block(&state, &vals, &Transport::default());
                    block.store((data.len() / ch) as u32, Relaxed);
                    for frame in data.chunks_mut(ch) {
                        let f = engine.next_frame(&state, &vals);
                        if ch == 1 {
                            frame[0] = T::from_sample((0.5 * (f.left + f.right)).clamp(-1.0, 1.0));
                            continue;
                        }
                        for (i, x) in frame.iter_mut().enumerate() {
                            // whole-number formats can't go past full scale: clip there
                            let v = if i == left {
                                f.left
                            } else if i == right {
                                f.right
                            } else {
                                0.0
                            };
                            *x = T::from_sample(v.clamp(-1.0, 1.0));
                        }
                    }
                },
                move |e| {
                    if let Ok(mut f) = failed.lock() {
                        *f = Some(e.to_string());
                    }
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(stream)
    }

    fn wanted_midi(&self) -> Vec<String> {
        match &self.settings.midi_inputs {
            Some(v) => v.clone(),
            None => self.midi_ports.first().cloned().into_iter().collect(),
        }
    }

    /// Open the MIDI inputs that are switched on and close the others.
    fn sync_midi(&mut self) {
        if !self.live {
            return;
        }
        let want = self.wanted_midi();
        let ports = &self.midi_ports;
        self.midi_conns.retain(|(n, _)| want.contains(n) && ports.contains(n));
        for name in want {
            if self.midi_ports.contains(&name) && !self.midi_conns.iter().any(|(n, _)| *n == name) {
                if let Some(c) = open_midi(&name, self.midi_tx.clone()) {
                    self.midi_conns.push((name, c));
                }
            }
        }
    }

    fn failure(&self) -> Option<String> {
        self.failed.lock().ok().and_then(|f| f.clone())
    }
}

impl SetupHost for Devices {
    fn scale(&self, ctx: &egui::Context) -> f32 {
        let r = ctx.screen_rect();
        (r.width() / ui::W).min(r.height() / ui::H)
    }
    fn set_scale(&mut self, ctx: &egui::Context, scale: f32) {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(ui::W * scale, ui::H * scale)));
        self.settings.scale = Some(scale);
        self.settings.save();
    }
    fn devices(&self) -> Option<DeviceView> {
        let failed = self.failure();
        let silent = self.live
            && self.stream.is_some()
            && failed.is_none()
            && self.block.load(Relaxed) == 0
            && self.started.is_some_and(|t| t.elapsed().as_secs_f32() > 2.0);
        let pairs = (self.channels as usize / 2).max(1);
        Some(DeviceView {
            drivers: self.drivers.iter().map(|h| h.name().to_string()).collect(),
            driver: self.host_id().name().to_string(),
            outputs: self.outputs.clone(),
            output: self.output_name.clone(),
            sample_rates: self.rates.clone(),
            chosen_rate: self.settings.sample_rate,
            buffer_sizes: self.buffers.clone(),
            chosen_buffer: self.settings.buffer,
            pairs,
            pair: self.settings.pair.min(pairs - 1),
            running: self.stream.is_some() && failed.is_none() && !silent,
            status: match failed {
                Some(e) => format!("Stopped: {e}"),
                None if silent => "Open, but this device asks for no sound: is it plugged in and switched on?".into(),
                None => self.status.clone(),
            },
            running_rate: self.running_rate,
            block_frames: self.block.load(Relaxed),
            midi_inputs: self
                .midi_ports
                .iter()
                .map(|n| (n.clone(), self.midi_conns.iter().any(|(c, _)| c == n)))
                .collect(),
        })
    }
    fn change(&mut self, change: DeviceChange) {
        match change {
            DeviceChange::Driver(name) => {
                self.stream = None; // let go of the old driver first
                self.host = open_host(Some(&name));
                self.settings.driver = Some(name);
                self.settings.output = None;
                self.settings.sample_rate = None;
                self.settings.buffer = None;
                self.settings.pair = 0;
                self.scan();
                self.start();
            }
            DeviceChange::Output(name) => {
                self.settings.output = Some(name);
                self.settings.pair = 0;
                self.start();
            }
            DeviceChange::SampleRate(r) => {
                self.settings.sample_rate = r;
                self.start();
            }
            DeviceChange::Buffer(b) => {
                self.settings.buffer = b;
                self.start();
            }
            DeviceChange::Pair(p) => {
                self.settings.pair = p;
                self.start();
            }
            DeviceChange::Midi(name, on) => {
                let mut list = self.wanted_midi();
                list.retain(|n| *n != name);
                if on {
                    list.push(name);
                }
                self.settings.midi_inputs = Some(list);
                self.sync_midi();
            }
            DeviceChange::Refresh => {
                self.scan();
                if self.live && (self.stream.is_none() || self.failure().is_some() || self.block.load(Relaxed) == 0) {
                    self.start();
                }
                self.sync_midi();
                return;
            }
        }
        self.settings.save();
    }
}

/// Sample formats Ritmo-6 can write.
const FORMATS: [cpal::SampleFormat; 3] = [cpal::SampleFormat::F32, cpal::SampleFormat::I32, cpal::SampleFormat::I16];

/// The audio driver called `name` (WASAPI, ASIO...), or the system's.
fn open_host(name: Option<&str>) -> cpal::Host {
    name.and_then(|n| cpal::available_hosts().into_iter().find(|h| h.name() == n))
        .and_then(|id| cpal::host_from_id(id).ok())
        .unwrap_or_else(cpal::default_host)
}

/// The installed ASIO drivers, read without loading any (None for other drivers).
#[cfg(all(windows, feature = "asio"))]
fn asio_driver_names(host: &cpal::Host) -> Option<Vec<String>> {
    (host.id() == cpal::HostId::Asio).then(|| asio_sys::Asio::new().driver_names())
}

#[cfg(not(all(windows, feature = "asio")))]
fn asio_driver_names(_host: &cpal::Host) -> Option<Vec<String>> {
    None
}

fn open_midi(name: &str, tx: mpsc::Sender<Midi>) -> Option<midir::MidiInputConnection<()>> {
    let input = midir::MidiInput::new("Ritmo-6").ok()?;
    let port = input.ports().into_iter().find(|p| input.port_name(p).ok().as_deref() == Some(name))?;
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
    /// Shrink the 1.6x window once if the screen is too small for it.
    fit: bool,
    devices: Devices,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.ui.setup.open && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            let r = self.state.running.load(Relaxed);
            self.state.running.store(!r, Relaxed);
        }
        if self.fit {
            if let Some(screen) = ctx.input(|i| i.viewport().monitor_size) {
                self.fit = false;
                let s = (screen.x * 0.92 / (ui::W * ui::START_SCALE)).min(screen.y * 0.85 / (ui::H * ui::START_SCALE));
                if s < 1.0 {
                    let k = ui::START_SCALE * s.max(0.6);
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(ui::W * k, ui::H * k)));
                }
            }
        }
        ui::draw(ctx, self.store.as_ref(), &self.state, &mut self.ui, &mut self.devices);

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

/// The program's icon (the person's own artwork), shown in the title bar and the task bar.
fn app_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon-256.png")).ok()?;
    let img = img.to_rgba8();
    Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
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
        });

    // the size chosen in the SETUP window last time (not for pictures)
    let mut settings = Settings::load();
    // --driver <WASAPI|ASIO> starts on that audio driver (its first device), e.g. to check a card
    if let Some(d) = args.iter().position(|a| a == "--driver").and_then(|i| args.get(i + 1)) {
        if settings.driver.as_deref() != Some(d.as_str()) {
            settings.driver = Some(d.clone());
            settings.output = None;
            settings.pair = 0;
        }
    }
    let saved_scale = settings.scale.filter(|k| (0.4..=3.0).contains(k) && shot.is_none());
    let fit_screen = shot.is_none() && size.is_none() && saved_scale.is_none();
    let k = saved_scale.unwrap_or(ui::START_SCALE);
    let size = size.unwrap_or([ui::W * k, ui::H * k]);

    let open_module = args
        .iter()
        .position(|a| a == "--module")
        .and_then(|i| args.get(i + 1))
        .and_then(|m| ritmo6_core::params::VOICE_LETTERS.iter().position(|l| l.eq_ignore_ascii_case(m)));

    // --fx opens the FX panel at start; --flip <0..1> holds the panel part-way through its turn.
    let fx_open = args.iter().any(|a| a == "--fx");
    let flip = args
        .iter()
        .position(|a| a == "--flip")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<f32>().ok());

    // --setup <interface|audio|midi> opens the SETUP window on that page at start.
    let setup_page = args.iter().position(|a| a == "--setup").and_then(|i| args.get(i + 1)).map(|p| match p.as_str() {
        "audio" => Page::Audio,
        "midi" => Page::Midi,
        "about" => Page::About,
        _ => Page::Interface,
    });

    // --chain opens the CHAIN panel at start; --chain-demo also fills it (for pictures)
    let chain_open = args.iter().any(|a| a == "--chain" || a == "--chain-demo");
    let chain_demo = args.iter().any(|a| a == "--chain-demo");
    let chain_flip = args.iter().position(|a| a == "--chain-flip").and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<f32>().ok());

    let store = Arc::new(Store::new());
    let state = Arc::new(SharedState::default());
    if chain_demo {
        for (i, p) in [1u8, 3, 12, 1, 7, 14].iter().enumerate() {
            state.chain[i].store(*p, Relaxed);
        }
        state.chain_on.store(true, Relaxed);
    }
    let devices = Devices::new(store.clone(), state.clone(), settings, shot.is_none());

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(concat!("Ritmo-6 v", env!("CARGO_PKG_VERSION"), " — Geranium"))
        .with_inner_size(size)
        .with_min_inner_size([ui::W * 0.6, ui::H * 0.6]);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
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
                    u.fx_open = fx_open || flip.is_some();
                    u.fx_p = if fx_open { 1.0 } else { 0.0 };
                    u.fx_freeze = flip;
                    if let Some(p) = setup_page {
                        u.setup.show(p);
                    }
                    // --check-update: press the About page's button at start (for pictures)
                    if args.iter().any(|a| a == "--check-update") {
                        u.setup.check_updates();
                    }
                    u.chain_open = chain_open || chain_flip.is_some();
                    u.chain_freeze = chain_flip;
                    u.chain_p = if chain_open { 1.0 } else { 0.0 };
                    u
                },
                shot,
                frames: 0,
                requested: false,
                fit: fit_screen,
                devices,
            }))
        }),
    )
}
