//! Starting rhythm programs. The original ships with an empty sequencer (its grid memory starts
//! at zero, OCET00 0x10001210); these demo rhythms are this rebuild's own addition.

use crate::kits::Kit;
use crate::params::{NUM_CH, NUM_PROGS};

/// MINI kit: the first-day sounds the person liked (bass drum, snare, conga, clave, closed hat,
/// cymbal), stored as knob voltages for the MINI engine (src/classic.rs):
/// tune V = 5 + log2(Hz/440), decay V = 20/3 + log2(seconds), sweep = Pitch Decay Vol x 4.8 st,
/// shape = Phase Mod Level / 10, volumes = level x 10.
/// Order: tune, tone decay, tone vol, pitch decay, pitch decay vol, pm rate, pm level (shape),
/// noise vol, noise decay, filter freq, filter reso, filter decay, lp/hp.
pub const MINI_KIT: Kit = Kit {
    voices: [
        // A bass drum: 52 Hz, 24 st sweep over 35 ms, 420 ms, sine, a touch of noise 12 ms LP 2 kHz
        [1.919, 5.415, 10.0, 1.830, 5.0, 0.0, 0.0, 0.5, 0.286, 7.184, 0.0, 0.0, 0.0],
        // B snare: 190 Hz, 7 st over 18 ms, 130 ms, shape .3, noise 190 ms HP 1.4 kHz
        [3.789, 3.724, 5.5, 0.871, 1.458, 0.0, 3.0, 7.5, 4.271, 6.670, 1.5, 0.0, 10.0],
        // C conga: 150 Hz, 5 st over 50 ms, 260 ms, shape .1
        [3.448, 4.723, 9.0, 2.345, 1.042, 0.0, 1.0, 0.0, 1.023, 7.770, 0.0, 0.0, 0.0],
        // D clave: 1.9 kHz, 30 ms, shape .45, a little noise HP 3 kHz
        [7.110, 1.608, 8.0, 0.0, 0.0, 0.0, 4.5, 1.5, 0.608, 7.770, 4.0, 0.0, 10.0],
        // E closed hat: noise 50 ms HP 7.5 kHz with a faint 3.4 kHz square
        [7.950, 1.023, 0.8, 0.0, 0.0, 0.0, 10.0, 9.0, 2.345, 9.091, 2.5, 0.0, 10.0],
        // F cymbal: noise 700 ms HP 5.2 kHz with a 2.9 kHz square, 400 ms
        [7.720, 5.345, 1.2, 0.0, 0.0, 0.0, 10.0, 8.0, 6.152, 8.563, 2.0, 0.0, 10.0],
    ],
    // first-day mixer: level 0.8 squared = 0.64, centred, master 0.8 squared = 0.64, 120 BPM
    volume: [6.4; 6],
    pan: [0.0; 6],
    master: 6.4,
    tempo: 5.556,
};

/// Turn "x...x..." into a step bit mask (bit 0 = step 1).
pub fn pat(s: &str) -> u16 {
    let mut m = 0u16;
    for (i, ch) in s.chars().take(16).enumerate() {
        if ch == 'x' || ch == 'X' {
            m |= 1 << i;
        }
    }
    m
}

pub struct Program {
    pub rows: [u16; NUM_CH],
    pub length: u8,
}

/// The first-day rhythms, written for the MINI kit: A bass drum, B snare, C conga, D clave,
/// E closed hat, F cymbal.
pub fn default_programs() -> Vec<Program> {
    let mut v: Vec<Program> = Vec::with_capacity(NUM_PROGS);
    let p = |a: &str, b: &str, c: &str, d: &str, e: &str, f: &str, len: u8| Program {
        rows: [pat(a), pat(b), pat(c), pat(d), pat(e), pat(f)],
        length: len,
    };
    // 1 rock
    v.push(p("x.......x.x.....", "....x.......x...", "", "", "x.x.x.x.x.x.x.x.", "x...............", 16));
    // 2 disco
    v.push(p("x...x...x...x...", "....x.......x...", "", "...x......x.....", "..x...x...x...x.", "x...............", 16));
    // 3 bossa
    v.push(p("x..xx..xx..xx..x", "", "..x....x..x....x", "x..x..x...x..x..", "x.x.x.x.x.x.x.x.", "", 16));
    // 4 waltz (12 steps)
    v.push(p("x...........", "....x...x...", "", "", "x.x.x.x.x.x.", "", 12));
    // 5 slow rock 12/8
    v.push(p("x.....x.x...", "...x.....x..", "", "", "xxxxxxxxxxxx", "x...........", 12));
    // 6 electro
    v.push(p("x.....x...x.....", "....x.......x...", "......x..x....x.", "..x.....x..x....", "xxxxxxxxxxxxxxxx", "", 16));
    // 7 march
    v.push(p("x.......x.......", "x.xxx.x.x.xxx.x.", "", "", "", "x...............", 16));
    // 8 beguine
    v.push(p("x.....x.x.......", "", "x..x..x...x.x...", "...x......x.....", "x.x.x.x.x.x.x.x.", "", 16));
    while v.len() < NUM_PROGS {
        v.push(p("", "", "", "", "", "", 16));
    }
    v
}
