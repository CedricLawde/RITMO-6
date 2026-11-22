//! FX expansion: four shared stereo buses (PHASER, DISTORTION, DELAY, SPRING).
//! Every voice sends to the buses; each bus runs one processor and returns into the master mix.
//! All memory is allocated in `Fx::new` (never on the audio thread). No locks, no UI access.

use crate::params::{
    delay_division, delay_free_ms, fx_level_gain, phaser_hz, FxParam, DELAY_DIVISIONS, NUM_FX_BUSES,
    NUM_FX_PARAMS,
};
use std::f32::consts::{PI, TAU};

/// Raw knob values of the FX section, indexed by `FxParam::index`.
pub type FxValues = [f32; NUM_FX_PARAMS];

#[inline]
fn fv(v: &FxValues, p: FxParam) -> f32 {
    v[p.index()]
}

#[inline]
fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 {
        0.0
    } else {
        x
    }
}

/// One-pole low-pass (and high-pass by subtraction).
#[derive(Clone, Copy, Default)]
struct OnePole {
    z: f32,
}

impl OnePole {
    #[inline]
    fn coef(fc: f32, sr: f32) -> f32 {
        1.0 - (-TAU * fc.clamp(5.0, sr * 0.49) / sr).exp()
    }
    #[inline]
    fn lp(&mut self, x: f32, a: f32) -> f32 {
        self.z = flush(self.z + a * (x - self.z));
        self.z
    }
    #[inline]
    fn hp(&mut self, x: f32, a: f32) -> f32 {
        x - self.lp(x, a)
    }
}

/// Parameter smoother (about 15 ms), so knob moves and automation never click.
#[derive(Clone, Copy)]
struct Smooth {
    v: f32,
}

impl Smooth {
    fn new(v: f32) -> Self {
        Smooth { v }
    }
    #[inline]
    fn next(&mut self, target: f32, k: f32) -> f32 {
        self.v += (target - self.v) * k;
        self.v
    }
}

/// RBJ low-pass biquad (transposed direct form II).
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn lowpass(fc: f32, q: f32, sr: f32) -> Self {
        let w = TAU * fc / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Biquad {
            b0: (1.0 - c) / 2.0 / a0,
            b1: (1.0 - c) / a0,
            b2: (1.0 - c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = flush(self.b1 * x - self.a1 * y + self.z2);
        self.z2 = flush(self.b2 * x - self.a2 * y);
        y
    }
}

#[derive(Clone, Copy)]
struct Rng(u32);

impl Rng {
    #[inline]
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

// ------------------------------------------------------------------------------------------ PHASER

/// Four-stage phaser in the spirit of the Small Stone: four first-order all-pass stages swept
/// by a smooth LFO, summed with the input, COLOR feeding the output back (resonance), and a
/// slow random drift of the LFO rate for an organic, slightly imperfect movement.
struct Phaser {
    sr: f32,
    z: [[f32; 4]; 2],
    fb: [f32; 2],
    lfo: f32,
    drift: f32,
    drift_target: f32,
    drift_count: u32,
    rng: Rng,
    s: [Smooth; 5],
}

impl Phaser {
    fn new(sr: f32) -> Self {
        Phaser {
            sr,
            z: [[0.0; 4]; 2],
            fb: [0.0; 2],
            lfo: 0.0,
            drift: 0.0,
            drift_target: 0.0,
            drift_count: 0,
            rng: Rng(0x51A7_0001),
            s: [Smooth::new(0.4), Smooth::new(0.7), Smooth::new(0.4), Smooth::new(0.5), Smooth::new(0.7)],
        }
    }

    fn process(&mut self, x: [f32; 2], v: &FxValues, k: f32) -> [f32; 2] {
        let rate = self.s[0].next(fv(v, FxParam::PhRate), k);
        let depth = self.s[1].next(fv(v, FxParam::PhDepth), k);
        let color = self.s[2].next(fv(v, FxParam::PhColor), k);
        let mix = self.s[3].next(fv(v, FxParam::PhMix), k);
        let level = fx_level_gain(self.s[4].next(fv(v, FxParam::PhLevel), k));

        // analog drift: the rate wanders by up to +-4 % every ~0.4 s, smoothly
        if self.drift_count == 0 {
            self.drift_target = self.rng.next() * 0.04;
            self.drift_count = (self.sr * 0.4) as u32;
        }
        self.drift_count -= 1;
        self.drift += (self.drift_target - self.drift) * (1.0 / (self.sr * 0.3));

        self.lfo = (self.lfo + phaser_hz(rate) * (1.0 + self.drift) / self.sr).fract();
        let mut out = [0.0; 2];
        for ch in 0..2 {
            // the right channel runs a few degrees behind for a little width
            let ph = self.lfo + ch as f32 * 0.03;
            let m = 0.5 - 0.5 * (TAU * ph).cos();
            let fc = 140.0 * (2.0f32).powf(depth * m * 5.0);
            let t = (PI * fc / self.sr).tan();
            let a = (t - 1.0) / (t + 1.0);
            let mut y = x[ch] + (self.fb[ch] * color * 0.8).tanh();
            for st in 0..4 {
                let o = a * y + self.z[ch][st];
                self.z[ch][st] = flush(y - a * o);
                y = o;
            }
            self.fb[ch] = flush(y);
            let wet = 0.5 * (x[ch] + y);
            out[ch] = (x[ch] * (1.0 - mix) + wet * mix) * level;
        }
        out
    }
}

// -------------------------------------------------------------------------------------- DISTORTION

/// RAT-style distortion: pre-emphasis, op-amp gain whose bandwidth shrinks as the gain rises,
/// soft/hard diode clipping run at 4x the sample rate, the RAT FILTER (a low-pass from ~32 kHz
/// down to ~475 Hz) and FAT, which keeps the clean low end (below ~120 Hz) so kicks keep their
/// fundamental. Output is gain-compensated.
struct Distortion {
    sr: f32,
    up: [[Biquad; 2]; 2],
    down: [[Biquad; 2]; 2],
    prev: [f32; 2],
    pre: [OnePole; 2],
    opamp: [OnePole; 2],
    post: [OnePole; 2],
    low: [OnePole; 2],
    s: [Smooth; 5],
}

impl Distortion {
    fn new(sr: f32) -> Self {
        let lp = Biquad::lowpass((sr * 0.45).min(20000.0), 0.7071, sr * 4.0);
        Distortion {
            sr,
            up: [[lp, lp]; 2],
            down: [[lp, lp]; 2],
            prev: [0.0; 2],
            pre: [OnePole::default(); 2],
            opamp: [OnePole::default(); 2],
            post: [OnePole::default(); 2],
            low: [OnePole::default(); 2],
            s: [Smooth::new(0.5), Smooth::new(0.4), Smooth::new(0.4), Smooth::new(1.0), Smooth::new(0.6)],
        }
    }

    #[inline]
    fn clip(v: f32) -> f32 {
        // diode pair: soft knee that becomes hard near +-1, a touch asymmetric
        let p = 2.5;
        let y = v / (1.0 + v.abs().powf(p)).powf(1.0 / p);
        y + 0.04 * y * y
    }

    fn process(&mut self, x: [f32; 2], v: &FxValues, k: f32) -> [f32; 2] {
        let drive = self.s[0].next(fv(v, FxParam::DsDrive), k);
        let filter = self.s[1].next(fv(v, FxParam::DsFilter), k);
        let fat = self.s[2].next(fv(v, FxParam::DsFat), k);
        let mix = self.s[3].next(fv(v, FxParam::DsMix), k);
        let level = fx_level_gain(self.s[4].next(fv(v, FxParam::DsLevel), k));

        let gain = 1.0 + 1999.0 * drive * drive * drive;
        let sr4 = self.sr * 4.0;
        let a_pre = OnePole::coef(60.0 + (1.0 - fat) * 640.0, self.sr);
        let a_op = OnePole::coef((1.0e6 / gain).min(20000.0), sr4);
        let a_post = OnePole::coef(32000.0 * (475.0f32 / 32000.0).powf(filter), self.sr);
        let a_low = OnePole::coef(120.0, self.sr);
        let comp = 1.0 - 0.65 * drive.powf(0.6);

        let mut out = [0.0; 2];
        for ch in 0..2 {
            let low = self.low[ch].lp(x[ch], a_low);
            let pre = self.pre[ch].hp(x[ch], a_pre);
            // 4x oversampled gain + clipping
            let mut y = 0.0;
            for i in 0..4 {
                let t = (i + 1) as f32 / 4.0;
                let s = self.prev[ch] + (pre - self.prev[ch]) * t;
                let s = self.up[ch][0].run(s);
                let s = self.up[ch][1].run(s);
                let g = self.opamp[ch].lp(s * gain, a_op);
                let c = self.down[ch][0].run(Self::clip(g));
                y = self.down[ch][1].run(c);
            }
            self.prev[ch] = pre;
            let filtered = self.post[ch].lp(y, a_post);
            let wet = filtered * comp + low * fat * 1.2;
            out[ch] = (x[ch] * (1.0 - mix) + wet * mix) * level;
        }
        out
    }
}

// ------------------------------------------------------------------------------------------- DELAY

const MAX_DELAY_SECONDS: f32 = 2.2;

/// Three-mode delay. TAPE: wow and irregular flutter, tape saturation and progressive
/// high-frequency loss in the feedback loop, a little tape hiss on the repeats, glide when TIME moves.
/// BBD: dark, clock-dependent bandwidth, soft companding saturation, clock whine and hiss,
/// gently modulated time. MOD: clean repeats with chorus-like modulation and detuned echoes;
/// AGE adds vintage digital coloration (fewer bits, lower sample rate in the loop).
struct Delay {
    sr: f32,
    buf: [Vec<f32>; 2],
    w: usize,
    cur: [f32; 2],
    wow: f32,
    flutter: [f32; 2],
    lfo: f32,
    fb_lp: [OnePole; 2],
    fb_hp: [OnePole; 2],
    tone: [OnePole; 2],
    noise_lp: OnePole,
    hold: [f32; 2],
    hold_count: u32,
    env: f32,
    cur_out: f32,
    whine: f32,
    rng: Rng,
    walk: f32,
    s: [Smooth; 6],
}

impl Delay {
    fn new(sr: f32) -> Self {
        let len = (sr * MAX_DELAY_SECONDS) as usize + 8;
        Delay {
            sr,
            buf: [vec![0.0; len], vec![0.0; len]],
            w: 0,
            cur: [sr * 0.3; 2],
            wow: 0.0,
            flutter: [0.0; 2],
            lfo: 0.0,
            fb_lp: [OnePole::default(); 2],
            fb_hp: [OnePole::default(); 2],
            tone: [OnePole::default(); 2],
            noise_lp: OnePole::default(),
            hold: [0.0; 2],
            hold_count: 0,
            env: 0.0,
            cur_out: 0.0,
            whine: 0.0,
            rng: Rng(0xDE1A_7001),
            walk: 0.0,
            s: [Smooth::new(0.4), Smooth::new(0.6), Smooth::new(0.3), Smooth::new(0.3), Smooth::new(1.0), Smooth::new(0.7)],
        }
    }

    /// Cubic (Hermite) read `d` samples behind the write head.
    #[inline]
    fn read(buf: &[f32], w: usize, d: f32) -> f32 {
        let len = buf.len();
        let pos = w as f32 - d;
        let pos = if pos < 0.0 { pos + len as f32 } else { pos };
        let i = pos.floor() as usize;
        let f = pos - pos.floor();
        let at = |o: isize| buf[((i as isize + o).rem_euclid(len as isize)) as usize];
        let (xm1, x0, x1, x2) = (at(-1), at(0), at(1), at(2));
        let c = (x1 - xm1) * 0.5;
        let v = x0 - x1;
        let wv = c + v;
        let a = wv + v + (x2 - x0) * 0.5;
        let b = wv + a;
        ((a * f - b) * f + c) * f + x0
    }

    fn process(&mut self, x: [f32; 2], v: &FxValues, k: f32, bpm: f64) -> [f32; 2] {
        let feedback = self.s[0].next(fv(v, FxParam::DlFeedback), k);
        let tone = self.s[1].next(fv(v, FxParam::DlTone), k);
        let age = self.s[2].next(fv(v, FxParam::DlAge), k);
        let modu = self.s[3].next(fv(v, FxParam::DlMod), k);
        let mix = self.s[4].next(fv(v, FxParam::DlMix), k);
        let level = fx_level_gain(self.s[5].next(fv(v, FxParam::DlLevel), k));
        let mode = (fv(v, FxParam::DlMode).round() as i32).clamp(0, 2);
        let sync = fv(v, FxParam::DlSync) >= 0.5;
        let tv = fv(v, FxParam::DlTime);

        let max_d = self.buf[0].len() as f32 - 8.0;
        let target = if sync {
            let beats = DELAY_DIVISIONS[delay_division(tv)].1;
            (beats * 60.0 / bpm.max(20.0) * self.sr as f64) as f32
        } else {
            delay_free_ms(tv) * 0.001 * self.sr
        }
        .clamp(4.0, max_d);
        // time glide: tape slowest (pitch bends like a real tape), MOD quickest
        let glide = [0.35, 0.18, 0.08][mode as usize];
        let kt = 1.0 / (glide * self.sr);

        // level of what comes out of the repeats: the hiss follows it (about 50 dB under it at
        // full AGE), so it can never be heard on its own and dies with the echoes
        let echo = self.cur_out;
        self.env = (self.env * 0.9995).max(echo);
        let hiss = self.env * age;

        let ms = 0.001 * self.sr;
        self.lfo = (self.lfo + 0.9 / self.sr).fract();
        self.wow = (self.wow + 0.55 / self.sr).fract();
        self.walk = flush(self.walk + (self.rng.next() * 0.002 - self.walk * 0.0005));
        let time_s = self.cur[0] / self.sr;
        let fc_bbd = (4000.0 * (0.25 / time_s.max(0.02)).sqrt()).clamp(1500.0, 7000.0) * (1.0 - 0.45 * age);
        let fc_fb = match mode {
            0 => 7000.0 - 4800.0 * age,
            1 => fc_bbd,
            _ => 16000.0,
        };
        let a_fb = OnePole::coef(fc_fb, self.sr);
        let a_hp = OnePole::coef(if mode == 2 { 20.0 } else { 90.0 }, self.sr);
        let a_tone = OnePole::coef(1500.0 * (10.0f32).powf(tone), self.sr);
        // the loop stays just under unity even at maximum FEEDBACK: very long tails, but no
        // runaway that would keep the loop (and anything in it) going for ever
        let fb_gain = feedback * if mode == 2 { 0.95 } else { 0.98 };

        // MOD-mode digital coloration: fewer bits and held samples as AGE rises
        let hold_every = if mode == 2 { 1 + (age * 3.0) as u32 } else { 1 };
        let refresh = self.hold_count == 0;
        self.hold_count = (self.hold_count + 1) % hold_every;
        let bits = 16.0 - age * 8.0;
        let q = (2.0f32).powf(bits - 1.0);

        let mut out = [0.0; 2];
        let mut peak = 0.0f32;
        for ch in 0..2 {
            self.cur[ch] += (target - self.cur[ch]) * kt;
            let m = match mode {
                0 => {
                    // wow (slow) + flutter (faster, irregular)
                    self.flutter[ch] = flush(self.flutter[ch] * 0.999 + self.rng.next() * 0.02);
                    let wow = (TAU * self.wow).sin() * (1.0 + self.walk * 40.0);
                    let flut = (TAU * self.wow * 13.3 + ch as f32).sin() * 0.5 + self.flutter[ch];
                    ms * (wow * 2.5 * (modu + age * 0.5) + flut * 0.3 * (modu + age))
                }
                1 => ms * ((TAU * self.lfo).sin() * 1.2 * modu + self.walk * 10.0 * age),
                _ => {
                    let ph = self.lfo + ch as f32 * 0.5;
                    ms * (TAU * ph).sin() * 8.0 * modu
                }
            };
            let d = (self.cur[ch] + m).clamp(2.0, max_d);
            let y = Self::read(&self.buf[ch], self.w, d);

            let mut f = y;
            match mode {
                0 => {
                    // tape saturation: squashes loud repeats but never adds gain to quiet ones,
                    // so the loop can't run away
                    let drive = 1.0 + age * 2.0;
                    f = (f * drive).tanh() / drive;
                    f = self.fb_lp[ch].lp(f, a_fb);
                    f = self.fb_hp[ch].hp(f, a_hp);
                }
                1 => {
                    f = (f * 1.2).tanh() / 1.2;
                    f = self.fb_lp[ch].lp(f, a_fb);
                    f = self.fb_hp[ch].hp(f, a_hp);
                }
                _ => {
                    if refresh {
                        // rounded toward zero, so the fewer bits never trap a repeat in an
                        // endless low buzz: every pass through the loop gets smaller
                        self.hold[ch] = (f * q).trunc() / q;
                    }
                    f = self.hold[ch].clamp(-2.0, 2.0);
                }
            }
            let write = x[ch] + f * fb_gain;
            self.buf[ch][self.w] = flush(write.clamp(-4.0, 4.0));

            let mut wet = self.tone[ch].lp(y, a_tone);
            peak = peak.max(y.abs());
            // hiss on the repeats only (never fed back into the loop): tape hiss in TAPE, clock
            // whine and hiss in BBD, proportional to the echoes' own level
            if mode == 0 {
                wet += self.noise_lp.lp(self.rng.next(), 0.3) * 0.006 * hiss;
            } else if mode == 1 {
                self.whine = (self.whine + 11250.0 / self.sr).fract();
                wet += ((TAU * self.whine).sin() * 0.002 + self.rng.next() * 0.004) * hiss;
            }
            out[ch] = (x[ch] * (1.0 - mix) + wet * mix) * level;
        }
        self.w = (self.w + 1) % self.buf[0].len();
        self.cur_out = peak;
        out
    }
}

// ------------------------------------------------------------------------------------------ SPRING

const SPRING_STAGES: usize = 24;

/// One spring: a chain of all-pass filters (dispersion: the "chirp" and drip of a real
/// spring), a delay line, frequency-dependent damping and feedback.
struct SpringLine {
    ap: [f32; SPRING_STAGES],
    buf: Vec<f32>,
    w: usize,
    base_ms: f32,
    damp: OnePole,
    fb: f32,
    wob: f32,
}

impl SpringLine {
    fn new(sr: f32, base_ms: f32) -> Self {
        SpringLine {
            ap: [0.0; SPRING_STAGES],
            buf: vec![0.0; (sr * 0.09) as usize + 8],
            w: 0,
            base_ms,
            damp: OnePole::default(),
            fb: 0.0,
            wob: 0.0,
        }
    }

    #[inline]
    fn run(&mut self, x: f32, a: f32, len: f32, g: f32, a_damp: f32, sr: f32) -> f32 {
        let mut s = x + self.fb * g;
        for z in self.ap.iter_mut() {
            let o = a * s + *z;
            *z = flush(s - a * o);
            s = o;
        }
        let n = self.buf.len();
        self.buf[self.w] = flush(s);
        // a very slow wobble of the length keeps the tank from ringing statically
        self.wob = (self.wob + 0.37 / sr).fract();
        let d = (len * (1.0 + 0.002 * (TAU * self.wob).sin())).clamp(2.0, (n - 3) as f32);
        let pos = self.w as f32 - d;
        let pos = if pos < 0.0 { pos + n as f32 } else { pos };
        let i = pos as usize;
        let f = pos - i as f32;
        let y = self.buf[i % n] * (1.0 - f) + self.buf[(i + 1) % n] * f;
        self.w = (self.w + 1) % n;
        let y = self.damp.lp(y, a_damp);
        self.fb = y;
        y
    }
}

/// Spring reverb modelled on a three-spring tank: nonlinear excitation (DRIVE), dispersive
/// all-pass chains, damped feedback lines of slightly different lengths, a thin low end like a
/// real tank, and a KICK impulse that knocks the tank. The output is soft-limited.
struct Spring {
    sr: f32,
    lines: [SpringLine; 3],
    pre_hp: OnePole,
    post_lp: [OnePole; 2],
    kick: f32,
    kick_phase: f32,
    rng: Rng,
    s: [Smooth; 6],
}

impl Spring {
    fn new(sr: f32) -> Self {
        Spring {
            sr,
            lines: [SpringLine::new(sr, 37.0), SpringLine::new(sr, 41.0), SpringLine::new(sr, 47.0)],
            pre_hp: OnePole::default(),
            post_lp: [OnePole::default(); 2],
            kick: 0.0,
            kick_phase: 0.0,
            rng: Rng(0x5B21_6001),
            s: [Smooth::new(0.5), Smooth::new(0.4), Smooth::new(0.5), Smooth::new(0.3), Smooth::new(1.0), Smooth::new(0.6)],
        }
    }

    fn knock(&mut self) {
        self.kick = 1.0;
        self.kick_phase = 0.0;
    }

    fn process(&mut self, x: [f32; 2], v: &FxValues, k: f32) -> [f32; 2] {
        let decay = self.s[0].next(fv(v, FxParam::SpDecay), k);
        let damping = self.s[1].next(fv(v, FxParam::SpDamping), k);
        let tension = self.s[2].next(fv(v, FxParam::SpTension), k);
        let drive = self.s[3].next(fv(v, FxParam::SpDrive), k);
        let mix = self.s[4].next(fv(v, FxParam::SpMix), k);
        let level = fx_level_gain(self.s[5].next(fv(v, FxParam::SpLevel), k));

        let mut input = 0.5 * (x[0] + x[1]);
        // KICK: a short, level-limited thump + rattle that excites the tank
        if self.kick > 0.0001 {
            self.kick_phase += 55.0 / self.sr;
            let thump = (TAU * self.kick_phase).sin() * self.kick;
            input += (0.5 * thump + 0.35 * self.rng.next() * self.kick * self.kick).clamp(-0.6, 0.6);
            self.kick *= (-1.0 / (0.03 * self.sr)).exp();
        }
        let excite = (input * (1.0 + drive * 6.0)).tanh() / (1.0 + drive * 1.5);
        let excite = self.pre_hp.hp(excite, OnePole::coef(160.0, self.sr));

        let a = 0.55 + 0.25 * tension;
        let len_scale = 1.35 - 0.6 * tension;
        let g = 0.55 + 0.42 * decay;
        let a_damp = OnePole::coef(9000.0 * (0.17f32).powf(damping), self.sr);
        let mut o = [0.0; 3];
        for (i, line) in self.lines.iter_mut().enumerate() {
            let len = line.base_ms * len_scale * 0.001 * self.sr;
            o[i] = line.run(excite, a, len, g, a_damp, self.sr);
        }
        let a_post = OnePole::coef(5500.0, self.sr);
        let wet_l = self.post_lp[0].lp(o[0] + 0.5 * o[2], a_post);
        let wet_r = self.post_lp[1].lp(o[1] - 0.5 * o[2], a_post);
        let wet = [(wet_l * 1.4).tanh(), (wet_r * 1.4).tanh()];
        [
            (x[0] * (1.0 - mix) + wet[0] * mix) * level,
            (x[1] * (1.0 - mix) + wet[1] * mix) * level,
        ]
    }
}

// --------------------------------------------------------------------------------------------- BUS

pub struct Fx {
    sr: f32,
    k_smooth: f32,
    phaser: Phaser,
    dist: Distortion,
    delay: Delay,
    spring: Spring,
    quiet: [u32; NUM_FX_BUSES],
    idle_after: u32,
}

impl Fx {
    /// Allocates every buffer for this sample rate. Call outside the audio thread.
    pub fn new(sr: f32) -> Self {
        Fx {
            sr,
            k_smooth: 1.0 - (-1.0 / (0.015 * sr)).exp(),
            phaser: Phaser::new(sr),
            dist: Distortion::new(sr),
            delay: Delay::new(sr),
            spring: Spring::new(sr),
            quiet: [0; NUM_FX_BUSES],
            idle_after: (0.5 * sr) as u32,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// KICK button: knock the spring tank.
    pub fn kick_spring(&mut self) {
        self.spring.knock();
        self.quiet[3] = 0;
    }

    /// One stereo sample: `bus[k]` is what the voices send to bus k; returns the summed
    /// returns of the four processors. Buses never feed each other. A bypassed processor is
    /// skipped and returns silence.
    #[inline]
    pub fn process(&mut self, bus: &[[f32; 2]; NUM_FX_BUSES], v: &FxValues, bpm: f64) -> [f32; 2] {
        let k = self.k_smooth;
        let mut out = [0.0f32; 2];
        let mut add = |r: [f32; 2]| {
            out[0] += r[0];
            out[1] += r[1];
        };
        let bypass = [FxParam::PhBypass, FxParam::DsBypass, FxParam::DlBypass, FxParam::SpBypass];
        for b in 0..NUM_FX_BUSES {
            if fv(v, bypass[b]) >= 0.5 {
                continue;
            }
            // a processor with nothing coming in and nothing left ringing for half a second
            // sleeps until something arrives (saves CPU when its sends are all at zero)
            let input_zero = bus[b][0] == 0.0 && bus[b][1] == 0.0;
            if input_zero && self.quiet[b] >= self.idle_after {
                continue;
            }
            let r = match b {
                0 => self.phaser.process(bus[0], v, k),
                1 => self.dist.process(bus[1], v, k),
                2 => self.delay.process(bus[2], v, k, bpm),
                _ => self.spring.process(bus[3], v, k),
            };
            if input_zero && r[0].abs() < 1e-7 && r[1].abs() < 1e-7 {
                self.quiet[b] = self.quiet[b].saturating_add(1);
            } else {
                self.quiet[b] = 0;
            }
            add(r);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{fx_spec, FX_PARAMS};

    fn defaults() -> FxValues {
        std::array::from_fn(|i| fx_spec(FX_PARAMS[i]).default)
    }

    #[test]
    fn silent_buses_stay_silent() {
        let mut fx = Fx::new(48000.0);
        let v = defaults();
        for _ in 0..48000 {
            let o = fx.process(&[[0.0; 2]; 4], &v, 120.0);
            assert!(o[0].abs() < 1e-6 && o[1].abs() < 1e-6);
        }
    }

    #[test]
    fn every_processor_is_stable_at_extremes() {
        let sr = 48000.0;
        for mode in 0..3 {
            let mut fx = Fx::new(sr);
            let mut v = defaults();
            for p in FX_PARAMS {
                let s = fx_spec(p);
                if s.show == crate::params::Show::Bypass {
                    continue;
                }
                v[p.index()] = s.max;
            }
            v[FxParam::DlMode.index()] = mode as f32;
            v[FxParam::DlSync.index()] = 0.0;
            let mut peak = 0.0f32;
            let mut rng = Rng(7);
            for i in 0..(sr as usize * 4) {
                let s = if i < 4800 { rng.next() * 0.8 } else { 0.0 };
                if i == 10000 {
                    fx.kick_spring();
                }
                let o = fx.process(&[[s, s]; 4], &v, 120.0);
                assert!(o[0].is_finite() && o[1].is_finite());
                peak = peak.max(o[0].abs()).max(o[1].abs());
            }
            assert!(peak < 12.0, "mode {mode}: peak {peak}");
        }
    }

    #[test]
    fn delay_repeats_at_the_set_time() {
        let sr = 48000.0;
        let mut fx = Fx::new(sr);
        let mut v = defaults();
        v[FxParam::PhBypass.index()] = 1.0;
        v[FxParam::DsBypass.index()] = 1.0;
        v[FxParam::SpBypass.index()] = 1.0;
        v[FxParam::DlMode.index()] = 2.0; // MOD, clean
        v[FxParam::DlMod.index()] = 0.0;
        v[FxParam::DlAge.index()] = 0.0;
        v[FxParam::DlSync.index()] = 1.0;
        v[FxParam::DlTime.index()] = 8.0 / 12.0; // 1/4 at 120 BPM = 500 ms
        v[FxParam::DlTone.index()] = 1.0;
        // let the time glide settle, then send one click
        for _ in 0..(sr as usize * 2) {
            fx.process(&[[0.0; 2]; 4], &v, 120.0);
        }
        let mut best = (0usize, 0.0f32);
        for i in 0..(sr as usize) {
            let s = if i == 0 { 1.0 } else { 0.0 };
            let o = fx.process(&[[0.0; 2], [0.0; 2], [s, s], [0.0; 2]], &v, 120.0);
            if i > 100 && o[0].abs() > best.1 {
                best = (i, o[0].abs());
            }
        }
        assert!((best.0 as f32 - 24000.0).abs() < 60.0, "echo at {} samples", best.0);
    }

    #[test]
    fn delay_at_full_feedback_dies_away_without_leftover_noise() {
        let sr = 48000.0;
        for mode in 0..3 {
            let mut fx = Fx::new(sr);
            let mut v = defaults();
            v[FxParam::PhBypass.index()] = 1.0;
            v[FxParam::DsBypass.index()] = 1.0;
            v[FxParam::SpBypass.index()] = 1.0;
            v[FxParam::DlMode.index()] = mode as f32;
            v[FxParam::DlFeedback.index()] = 1.0;
            v[FxParam::DlAge.index()] = 1.0;
            v[FxParam::DlMix.index()] = 1.0;
            v[FxParam::DlSync.index()] = 1.0;
            v[FxParam::DlTime.index()] = 6.0 / 12.0;
            let mut first = 0.0f32;
            let mut last = 0.0f32;
            let total = sr as usize * 40;
            for i in 0..total {
                // a burst of tone (no noise at all in the input)
                let s = if i < 2400 { (i as f32 * 0.05).sin() * 0.5 } else { 0.0 };
                let o = fx.process(&[[0.0; 2], [0.0; 2], [s, s], [0.0; 2]], &v, 120.0);
                if i < sr as usize * 2 {
                    first = first.max(o[0].abs());
                }
                if i > total - sr as usize {
                    last = last.max(o[0].abs());
                }
            }
            assert!(last < first * 0.05, "mode {mode}: still {last} after 40 s (first {first})");
        }
    }

    #[test]
    fn a_sleeping_spring_still_answers_the_kick() {
        let sr = 48000.0;
        let mut fx = Fx::new(sr);
        let v = defaults();
        for _ in 0..(sr as usize * 2) {
            fx.process(&[[0.0; 2]; 4], &v, 120.0);
        }
        assert!(fx.quiet[3] >= fx.idle_after, "the idle spring should be asleep");
        fx.kick_spring();
        let mut peak = 0.0f32;
        for _ in 0..(sr as usize / 2) {
            let o = fx.process(&[[0.0; 2]; 4], &v, 120.0);
            peak = peak.max(o[0].abs()).max(o[1].abs());
        }
        assert!(peak > 0.01, "the kick should be heard, peak {peak}");
    }
}
