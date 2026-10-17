//! AULA's own music modes, ported from its Windows driver (OemDrv.exe, decompiled): the same
//! 23-band levels, gain and smoothing, key groups, colour cycles and frame logic (one step every
//! 20 ms), streamed as live frames.
use crate::fx::Colours;
use crate::layout::KEYS;
use crate::protocol::LED_COUNT;
use std::sync::OnceLock;

pub const BANDS: usize = 23;
/// One FFT bin per band (1024-point FFT of the left channel, no window).
pub const BAND_HZ: [f64; BANDS] = [
    30.0, 60.0, 100.0, 160.0, 240.0, 300.0, 350.0, 400.0, 440.0, 500.0, 600.0, 800.0, 1000.0, 1500.0, 2000.0, 2600.0, 3000.0,
    4000.0, 6000.0, 8000.0, 10000.0, 14000.0, 16000.0,
];
/// Mode names as AULA's app shows them.
pub const NAMES: [&str; 10] = [
    "Audio dance - soft",
    "Dazzling - rock",
    "Clouds rise and snow fly - routine",
    "Light Field Change - voice",
    "The gurgling stream - regular",
    "Blooming - passion",
    "Pearl falling jade plate - rock",
    "Clouds follow the moon - passion",
    "Mountains and Flowing Waters - regular",
    "Raining like silk - regular",
];
pub const GAIN_STEPS: u8 = 16;
pub const DEFAULT_GAIN: u8 = 5; // x1.0
pub const DEFAULT_SMOOTH: u8 = 4;
pub const DEFAULT_COLOUR: [u8; 3] = [0, 255, 0];

/// Gain factor for a slider step: 0.5 .. 2.0 in 0.1 steps.
pub fn gain(i: u8) -> f64 {
    0.5 + 0.1 * i.min(GAIN_STEPS - 1) as f64
}

/// Band level before gain: 20*log10 of the bin power (full scale = 1.0), truncated, at least 0.
pub fn raw_level(power: f32) -> i32 {
    if power > 0.0 {
        ((20.0 * (power as f64).log10()) as i32).max(0)
    } else {
        0
    }
}

/// The levels the modes see: gain, clamp to 100, then the integer mean of the last `smooth` reads.
pub fn levels(history: &[[i32; BANDS]], gain_step: u8, smooth: u8) -> [i32; BANDS] {
    let g = gain(gain_step);
    let n = (smooth.max(1) as usize).min(history.len().max(1));
    let mut out = [0; BANDS];
    for (b, o) in out.iter_mut().enumerate() {
        let sum: i32 = history.iter().rev().take(n).map(|h| ((h[b].max(0) as f64 * g) as i32).min(100)).sum();
        *o = sum / n as i32;
    }
    out
}

// Colours are kept the way the driver keeps them: 0x00BBGGRR.
pub fn colorref(c: [u8; 3]) -> u32 {
    c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16
}

fn rgb(c: u32) -> [u8; 3] {
    [c as u8, (c >> 8) as u8, (c >> 16) as u8]
}

fn scale(c: u32, num: i32, den: i32) -> u32 {
    let ch = |s: u32| (((c >> s & 0xff) as i32 * num / den) as u32 & 0xff) << s;
    ch(0) | ch(8) | ch(16)
}

fn lerp(from: u32, to: u32, total: i32, pos: i32) -> u32 {
    let ch = |s: u32| {
        let (a, b) = ((from >> s & 0xff) as f32, (to >> s & 0xff) as f32);
        ((pos as f32 * ((b - a) / total as f32) + a).round_ties_even() as i32 as u32 & 0xff) << s
    };
    ch(0) | ch(8) | ch(16)
}

/// How many of `n` steps a 0..100 level lights, and the leftover that dims the last one.
fn fill(level: i32, n: i32) -> (i32, i32) {
    let step = (100.0f64 / n as f64) as f32;
    let x = level as f32 / step;
    let full = x as i32;
    let rem = ((x - full as f32) * step) as i32;
    (if rem > 0 && full < n { full + 1 } else { full }, rem)
}

const PAL8: [u32; 8] = [0xff, 0xff00ff, 0xffff, 0xffff00, 0xff0000, 0xff00, 0x80ff, 0x8000];

// ------------------------------------------------------------------ key groups

/// Columns for "Audio dance" and the scrolling mode: each column lists its rows bottom to top.
const BARS: &[&[&[&str]]] = &[
    &[&["ctrl", "win"], &["shift"], &["caps"], &["tab", "q"], &["`", "1"], &["esc"]],
    &[&["alt"], &["z", "x"], &["a", "s"], &["q", "w"], &["2", "3"], &["f1", "f2"]],
    &[&["c", "v"], &["d", "f"], &["e", "r"], &["4", "5"], &["f3", "f4"]],
    &[&["space"], &["b"], &["g", "h"], &["t", "y"], &["6", "7"], &["f5", "f6"]],
    &[&["n", "m"], &["j", "k"], &["u", "i"], &["8", "9"], &["f7"]],
    &[&["ralt"], &[",", "."], &["l"], &["o", "p"], &["0"], &["f8"]],
    &[&["fn"], &["/"], &[";", "'"], &["p", "["], &["-", "="], &["f9", "f10"]],
    &[&["left", "down", "rctrl"], &["rshift", "up"], &["enter"], &["]", "\\"], &["back"], &["f11", "f12"]],
    &[&["right"], &["end"], &["pgdn"], &["pgup"], &["del"], &["home"]],
];

/// Three diagonal fans (left, middle, right), each listed from its corner outwards.
const FANS: &[&[&[&str]]] = &[
    &[&["ctrl", "win"], &["shift", "alt"], &["caps", "z"], &["tab", "a", "x"], &["`", "1", "q", "s", "c"], &["esc", "f1", "2", "w", "d", "v"]],
    &[
        &["space", "ralt", "fn"],
        &["c", "v", "b", "n", "m", ",", "."],
        &["f", "g", "h", "j", "k", "l"],
        &["t", "y", "u", "i", "o"],
        &["6", "7", "8", "9"],
        &["f5", "f6", "f7"],
    ],
    &[&["right"], &["end", "down"], &["pgdn", "up", "left"], &["pgup", "enter", "rshift", "rctrl"], &["del", "\\"], &["back", "]", "'", "/"]],
];

/// Rings around U, inside out.
const RINGS: &[&[&str]] = &[
    &["u"],
    &["7", "8", "y", "i", "h", "j"],
    &["f6", "6", "t", "g", "b", "n", "m", "k", "o", "9"],
    &["f5", "5", "r", "f", "v", "space", ",", "l", "p", "0", "f7"],
    &["f4", "4", "e", "d", "c", ".", ";", "[", "-", "f8"],
    &["f3", "3", "w", "s", "x", "ralt", "/", "'", "]", "=", "f9"],
    &["f2", "2", "q", "a", "z", "alt", "fn", "app", "rctrl", "rshift", "enter", "\\", "back", "f10", "f11"],
    &["f1", "1", "tab", "caps", "shift", "win", "left", "up", "pgdn", "pgup", "home", "f12"],
];

/// Vertical strips left to right, used for the flowing ColorFull colours.
const STRIPS: &[&[&str]] = &[
    &["esc", "`", "tab", "caps", "shift", "ctrl", "win"],
    &["1", "q", "a", "z", "alt"],
    &["f1", "2", "w", "s", "x"],
    &["f2", "3", "e", "d", "c"],
    &["f3", "4", "r", "f", "v"],
    &["f4", "5", "t", "g", "b", "space"],
    &["6", "y", "h", "n"],
    &["f5", "7", "u", "j", "m"],
    &["f6", "8", "i", "k", ","],
    &["f7", "9", "o", "l", ".", "ralt"],
    &["f8", "0", "p", ";", "/", "fn"],
    &["f9", "-", "[", "'", "app", "f10", "=", "]"],
    &["left", "down", "rctrl", "rshift", "up", "enter", "]", "\\", "back", "f11", "f12"],
    &["right", "end", "pgdn", "pgup", "del"],
];

/// Windows virtual-key code for a key name (names for keys the F75 lacks resolve to nothing).
fn vk(name: &str) -> Option<u8> {
    let b = name.as_bytes();
    if b.len() == 1 && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit()) {
        return Some(b[0].to_ascii_uppercase());
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return Some(0x6F + n);
    }
    Some(match name {
        "`" => 0xC0,
        "-" => 0xBD,
        "=" => 0xBB,
        "[" => 0xDB,
        "]" => 0xDD,
        "\\" => 0xDC,
        ";" => 0xBA,
        "'" => 0xDE,
        "," => 0xBC,
        "." => 0xBE,
        "/" => 0xBF,
        "esc" => 0x1B,
        "tab" => 0x09,
        "caps" => 0x14,
        "shift" => 0xA0,
        "rshift" => 0xA1,
        "ctrl" => 0xA2,
        "rctrl" => 0xA3,
        "alt" => 0xA4,
        "ralt" => 0xA5,
        "win" => 0x5B,
        "app" => 0x5D,
        "fn" => 0xFA,
        "space" => 0x20,
        "enter" => 0x0D,
        "back" => 0x08,
        "del" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "pgup" => 0x21,
        "pgdn" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        _ => return None,
    })
}

fn leds(names: &[&str]) -> Vec<usize> {
    names.iter().filter_map(|n| vk(n)).filter_map(|v| KEYS.iter().find(|k| k.vk == v)).map(|k| k.led as usize).collect()
}

struct Geo {
    keys: Vec<(usize, [i32; 4])>, // (led, l t r b) in layout order
    rows: Vec<Vec<usize>>,        // the six keyboard rows, left to right
    bars: Vec<Vec<Vec<usize>>>,
    fans: Vec<Vec<Vec<usize>>>,
    rings: Vec<Vec<usize>>,
    strips: Vec<Vec<usize>>,
    w: i32,
    h: i32,
    key_h: i32,
    wave_x: i32,  // left edge of the wave
    mid_y: i32,   // vertical middle of the keys
    wave_r: i32,  // wave amplitude at level 100
}

fn geo() -> &'static Geo {
    static G: OnceLock<Geo> = OnceLock::new();
    G.get_or_init(|| {
        let keys: Vec<(usize, [i32; 4])> = KEYS.iter().map(|k| (k.led as usize, k.rect.map(|v| v as i32))).collect();
        let x0 = keys.iter().map(|k| k.1[0]).min().unwrap();
        let y0 = keys.iter().map(|k| k.1[1]).min().unwrap();
        let w = keys.iter().map(|k| k.1[2]).max().unwrap() - x0;
        let h = keys.iter().map(|k| k.1[3]).max().unwrap() - y0;
        let key_h = keys[0].1[3] - keys[0].1[1]; // Esc
        // a row = every key that spans the vertical middle of its reference key, sorted by x
        let rows = ["f1", "1", "q", "a", "x", "space"]
            .iter()
            .map(|n| {
                let r = KEYS.iter().find(|k| Some(k.vk) == vk(n)).unwrap().rect.map(|v| v as i32);
                let mid = (r[3] - r[1]) / 2 + r[1];
                let mut row: Vec<&(usize, [i32; 4])> = keys.iter().filter(|k| k.1[1] <= mid && mid <= k.1[3]).collect();
                row.sort_by_key(|k| k.1[0]);
                row.iter().map(|k| k.0).collect()
            })
            .collect();
        let grid = |g: &[&[&[&str]]]| g.iter().map(|col| col.iter().map(|cell| leds(cell)).collect()).collect();
        Geo {
            keys,
            rows,
            bars: grid(BARS),
            fans: grid(FANS),
            rings: RINGS.iter().map(|r| leds(r)).collect(),
            strips: STRIPS.iter().map(|s| leds(s)).collect(),
            w,
            h,
            key_h,
            wave_x: (x0 as f32 + 5.0) as i32,
            mid_y: (h as f32 * 0.5 + y0 as f32) as i32,
            wave_r: (h as f32 * 0.5 + key_h as f32) as i32,
        }
    })
}

/// The key under a point; the space bar only counts in a key-wide strip at its centre.
fn key_at(x: i32, y: i32) -> Option<usize> {
    let g = geo();
    let key_w = g.keys[0].1[2] - g.keys[0].1[0];
    g.keys.iter().find_map(|&(led, [l, t, r, b])| {
        let (l, r) = if KEYS.iter().any(|k| k.led as usize == led && k.vk == 0x20) {
            let l = l + ((r - l) - key_w) / 2;
            (l, l + key_w)
        } else {
            (l, r)
        };
        (l <= x && x < r && t <= y && y < b).then_some(led)
    })
}

// ------------------------------------------------------------------ ColorFull

/// ColorFull: each key group holds a palette colour, then fades to the next; groups start
/// staggered so the colours travel across the keyboard.
struct Cycle {
    pal: &'static [u32],
    hold: i32,
    fade: i32,
    groups: &'static [Vec<usize>],
    t: Vec<i32>,
    k: Vec<i32>,
}

const STEP: i32 = 30;

impl Cycle {
    fn strips() -> Self {
        Self::new(&[200, 0xff0000, 0xff00], STEP * 4, STEP * 15, &geo().strips, STEP * 4)
    }

    fn rings() -> Self {
        let n = geo().rings.len() as i32;
        Self::new(&[0xff, 0xff00ff, 0xffff, 0xff00, 0xff0000, 0xffff00], STEP * 4, STEP * 8, &geo().rings, STEP * 72 / n / 2)
    }

    fn new(pal: &'static [u32], hold: i32, fade: i32, groups: &'static [Vec<usize>], stagger: i32) -> Self {
        let total = pal.len() as i32 * (hold + fade);
        let mut t = vec![0; groups.len()];
        let mut v = stagger; // the slot after the last group starts at 0
        for g in (0..groups.len()).rev() {
            if v >= total {
                v = 0;
            }
            t[g] = v;
            v += stagger;
        }
        Self { pal, hold, fade, groups, t, k: vec![0; groups.len()] }
    }

    fn step(&mut self, table: &mut [u32; LED_COUNT]) {
        let period = self.hold + self.fade;
        let total = self.pal.len() as i32 * period;
        for (g, keys) in self.groups.iter().enumerate() {
            if self.t[g] >= total {
                self.t[g] = 0;
            }
            let (t, i) = (self.t[g], (self.t[g] / period) as usize);
            let c = if t % period < self.hold {
                self.k[g] = 0;
                self.pal[i]
            } else {
                let c = lerp(self.pal[i], self.pal[(i + 1) % self.pal.len()], self.fade, self.k[g]);
                self.k[g] += STEP;
                c
            };
            self.t[g] = t + STEP;
            for &l in keys {
                table[l] = c;
            }
        }
    }
}

// ------------------------------------------------------------------ the modes

pub struct Music {
    mode: Option<u8>,
    cycle: Cycle,
    table: [u32; LED_COUNT], // per-key ColorFull colour
    refresh: i32,            // frames since the colours last moved
    skip: i32,               // modes that only update every other frame
    last: i32,               // previous level, for beat detection / falling peaks
    rising: bool,
    history: [i32; BANDS],
    burst: i32,              // circle size limit, or flash frames left
    circle: [i32; 4],        // x, y, size, growth
    circle_colour: u32,
    runs: [[i32; 4]; 6],     // per row: start, length, position, kind
    held: i32,
    held_colour: u32,
    wave: i32,
    seed: u32,
    out: Option<Colours>,
}

impl Default for Music {
    fn default() -> Self {
        Self {
            mode: None,
            cycle: Cycle::strips(),
            table: [0; LED_COUNT],
            refresh: 0,
            skip: 0,
            last: 0,
            rising: false,
            history: [0; BANDS],
            burst: 0,
            circle: [0; 4],
            circle_colour: 0,
            runs: [[0; 4]; 6],
            held: 0,
            held_colour: 0,
            wave: 360,
            seed: 0x2545_f491,
            out: None,
        }
    }
}

impl Music {
    fn rand(&mut self) -> i32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8 & 0x7fff) as i32
    }

    fn reset(&mut self, mode: u8) {
        let seed = self.seed;
        *self = Self { seed, ..Self::default() };
        self.mode = Some(mode);
        self.cycle = if mode == 1 { Cycle::rings() } else { Cycle::strips() };
    }

    /// Moves the ColorFull colours every `every` frames.
    fn tick_colours(&mut self, colorful: bool, every: i32) {
        if colorful {
            self.refresh += 1;
            if self.refresh >= every {
                self.refresh = 0;
                self.cycle.step(&mut self.table);
            }
        }
    }

    /// A beat: the level stops rising while above `min`.
    fn beat(&mut self, level: i32, min: i32) -> bool {
        let rising = self.last < level;
        let beat = self.rising && !rising && level > min;
        self.rising = rising;
        self.last = level;
        beat
    }

    /// One 20 ms step. Returns the frame to show (the previous frame when a mode skips a step).
    pub fn step(&mut self, mode: u8, lv: &[i32; BANDS], colorful: bool, single: u32) -> Colours {
        if self.mode != Some(mode) {
            self.reset(mode);
        }
        let mut f = [0u32; LED_COUNT];
        let drawn = lv.iter().all(|&v| v == 0) || self.draw(mode, lv, colorful, single, &mut f);
        if drawn {
            self.out = Some(f.map(rgb));
        }
        self.out.unwrap_or([[0; 3]; LED_COUNT])
    }

    fn draw(&mut self, mode: u8, lv: &[i32; BANDS], colorful: bool, single: u32, f: &mut [u32; LED_COUNT]) -> bool {
        let g = geo();
        match mode {
            0 | 4 | 6 => {
                if mode == 4 {
                    self.tick_colours(colorful, 2);
                    self.history.copy_within(0..BANDS - 1, 1);
                    self.history[0] = lv[1];
                } else {
                    self.tick_colours(colorful, 5);
                }
                let table = self.table;
                let col = |l: usize| if colorful { table[l] } else { single };
                let hist = self.history;
                let (groups, level): (&[Vec<Vec<usize>>], Box<dyn Fn(usize) -> i32>) = match mode {
                    0 => (&g.bars, Box::new(|i| lv[i + 1])),
                    4 => (&g.bars, Box::new(move |i| hist[i])),
                    _ => (&g.fans, Box::new(|i| [lv[1], lv[1], lv[2], lv[1]][i])),
                };
                for (i, cells) in groups.iter().enumerate() {
                    let n = cells.len() as i32;
                    let (lit, rem) = fill(level(i).min(100), n);
                    for (r, cell) in cells.iter().enumerate().take(lit as usize) {
                        for &l in cell {
                            f[l] = if rem != 0 && r as i32 == lit - 1 { scale(col(l), rem, 100 / n) } else { col(l) };
                        }
                    }
                }
            }
            1 => {
                self.tick_colours(colorful, 2);
                let table = self.table;
                let col = |l: usize| if colorful { table[l] } else { single };
                let mut level = lv[1].min(100);
                if level < self.last {
                    self.last = (self.last - (self.last - level).min(5)).max(0);
                    level = self.last;
                } else {
                    self.last = level;
                }
                let n = g.rings.len() as i32;
                let (lit, rem) = fill(level, n);
                for (r, ring) in g.rings.iter().enumerate().take(lit as usize) {
                    for &l in ring {
                        f[l] = if rem != 0 && r as i32 == lit - 1 { scale(col(l), rem, 100 / n) } else { col(l) };
                    }
                }
            }
            2 => {
                self.tick_colours(colorful, 5);
                let table = self.table;
                let col = |l: usize| if colorful { table[l] } else { single };
                // the driver sizes every row by the top row's key count
                let n = g.rows[0].len() as i32;
                for (r, row) in g.rows.iter().enumerate() {
                    let (lit, rem) = fill(lv[r + 1].min(100), n);
                    for (i, &l) in row.iter().enumerate().take(n.min(lit) as usize) {
                        f[l] = if rem != 0 && i as i32 == lit - 1 { scale(col(l), rem, 100 / n) } else { col(l) };
                    }
                }
            }
            3 => {
                let mut level = lv[2];
                if level < 1 {
                    level = lv[12];
                }
                let level = level.min(100);
                let c = if colorful {
                    if level < 1 || level < self.held {
                        self.held = (self.held - (self.held - level).min(10)).max(0);
                    } else {
                        self.held_colour = match level {
                            ..=40 => 0xff0000,
                            41..=50 => 0xffff00,
                            51..=60 => 0xff00,
                            61..=70 => 0xffff,
                            71..=80 => 0x80ff,
                            81..=90 => 0xff00ff,
                            _ => 0xff,
                        };
                        self.held = level;
                    }
                    scale(self.held_colour, self.held, 100)
                } else {
                    scale(single, level, 100)
                };
                for &(l, _) in &g.keys {
                    f[l] = c;
                }
            }
            5 => {
                if self.beat(lv[1], 49) {
                    self.burst = lv[1] * (g.w as f32 / 150.0) as i32;
                    let i = (self.rand() % 8) as usize;
                    self.circle_colour = if colorful { PAL8[i] } else { single };
                    let cells = -4 - (g.w as f32 / -10.0 + 1.0) as i32;
                    let x = (self.rand() % cells + 5) * 10;
                    let y = self.rand() % 81 - 40 + g.mid_y;
                    self.circle = [x, y, 0, (self.burst / 5).max(1)];
                }
                if self.burst == 0 {
                    if self.skip < 2 {
                        self.skip += 1;
                        return false;
                    }
                    self.skip = 0;
                    let unit = (g.w as f32 / 100.0 * 0.5) as i32;
                    let (xs, ys) = ((g.w as f32 / 10.0 - 1.0) as i32, (g.h as f32 / 10.0) as i32 - 1);
                    for b in [lv[1], lv[2]] {
                        let s = b.min(100) * unit;
                        let x = (self.rand() % xs + 1) * 10;
                        let y = (self.rand() % ys + 2) * 10;
                        let i = (self.rand() % 8) as usize;
                        let c = if colorful { PAL8[i] } else { single };
                        for &(l, [kl, kt, kr, kb]) in &g.keys {
                            if x <= kl && kr <= x + s && y <= kt && kb <= y + s {
                                f[l] = c;
                            }
                        }
                    }
                } else {
                    let [x, y, size, grow] = self.circle;
                    let size = size + grow;
                    self.circle[2] = size;
                    let r = size / 2;
                    let inside = |px: i32, py: i32| {
                        r > 0 && {
                            let (dx, dy) = ((px - x) as f32 / r as f32, (py - y) as f32 / r as f32);
                            dx * dx + dy * dy < 1.0
                        }
                    };
                    for &(l, [kl, kt, kr, kb]) in &g.keys {
                        let n = [(kl, kt), (kr, kt), (kl, kb), (kr, kb)].iter().filter(|p| inside(p.0, p.1)).count();
                        let pct = [0, 20, 40, 70, 100][n];
                        f[l] = if pct == 0 { 0 } else { scale(self.circle_colour, pct, 100) };
                    }
                    if self.burst <= size {
                        self.burst = 0;
                    }
                }
            }
            7 => {
                if self.beat(lv[2], 59) && self.burst == 0 {
                    self.burst = 2;
                }
                self.tick_colours(colorful, 2);
                let table = self.table;
                let col = |l: usize| if colorful { table[l] } else { single };
                if self.skip < 1 {
                    self.skip += 1;
                    return false;
                }
                self.skip = 0;
                if self.burst == 0 {
                    for r in 0..g.rows.len() {
                        let level = lv[r + 1];
                        if self.runs[r][3] == 0 && level > 0 {
                            let cnt = g.rows[r].len() as i32;
                            let start = self.rand() % (cnt / 3 + 1);
                            let mut len = level * cnt / 100 + 1;
                            if cnt < len + start {
                                len = cnt - start - 1;
                            }
                            let kind = self.rand() % 3 + 1;
                            self.runs[r] = [start, len, 0, kind];
                        }
                    }
                    for (r, row) in g.rows.iter().enumerate() {
                        let [start, len, pos, kind] = self.runs[r];
                        match kind {
                            1 | 2 => {
                                for i in 0..len.min(pos) {
                                    if let Some(&l) = row.get((start + i) as usize) {
                                        f[l] = col(l);
                                    }
                                }
                                self.runs[r][2] = pos + if len < 7 { 1 } else { 2 };
                                if len < self.runs[r][2] {
                                    self.runs[r] = [0; 4];
                                    break; // the driver stops the row loop here
                                }
                            }
                            3 => {
                                for &l in row {
                                    f[l] = col(l);
                                }
                                self.runs[r] = [0; 4];
                            }
                            _ => {}
                        }
                    }
                } else {
                    for &(l, _) in &g.keys {
                        f[l] = col(l);
                    }
                    self.burst = (self.burst - 1).max(0);
                }
            }
            8 => {
                if self.beat(lv[1], 59) && self.burst == 0 {
                    self.burst = 2;
                }
                if self.burst != 0 {
                    self.burst = (self.burst - 1).max(0); // a dark frame on the beat
                    return true;
                }
                self.tick_colours(colorful, 5);
                let table = self.table;
                let col = |l: usize| if colorful { table[l] } else { single };
                if self.skip < 1 {
                    self.skip += 1;
                    return false;
                }
                self.skip = 0;
                for (r, row) in g.rows.iter().enumerate() {
                    let cnt = row.len();
                    let half = cnt / 2;
                    let raw = lv[r + 1];
                    let (lit, _) = fill(((raw as f64 * 1.3) as i32).min(100), half as i32);
                    if raw > 49 && self.runs[r][3] == 0 {
                        self.runs[r][3] = 1;
                        self.runs[r][2] = 0;
                    }
                    let outward = (0..=half).rev().chain(half + 1..cnt);
                    let dist = |i: usize| if i <= half { half - i } else { i - half - 1 } as i32;
                    if self.runs[r][3] == 0 {
                        if lit > 0 {
                            let sparkle = self.rand() % 3 == 2;
                            for i in outward {
                                if let Some(&l) = row.get(i) {
                                    f[l] = if dist(i) <= lit && sparkle { col(l) } else { 0 };
                                }
                            }
                        }
                    } else {
                        let pos = self.runs[r][2];
                        for i in outward {
                            if let Some(&l) = row.get(i) {
                                f[l] = if dist(i) <= pos { col(l) } else { 0 };
                            }
                        }
                        self.runs[r][2] += if cnt < 16 || half as i32 - 2 <= pos { 1 } else { 2 };
                        if (half as i32) < self.runs[r][2] {
                            self.runs[r][3] = 0;
                        }
                    }
                }
            }
            9 => {
                self.tick_colours(colorful, 5);
                let table = self.table;
                let col = |l: usize| if colorful { table[l] } else { single };
                let mut amp = lv[1] * g.wave_r / 100;
                if amp < g.key_h && lv[1] > 0 {
                    amp = g.key_h;
                }
                if amp > 0 {
                    let step = (g.key_h / 2).max(1);
                    let sx = g.w as f32 / 720.0;
                    let mut t = 0;
                    while t <= 720 {
                        let a = ((self.wave + t) as f32 * 0.017_453_292_5) as f32;
                        let x = (t as f32 * sx).floor() as i32 + g.wave_x;
                        let y = (amp as f32 * a.sin()).floor() as i32 + g.mid_y;
                        if let Some(l) = key_at(x, y) {
                            f[l] = col(l);
                        }
                        t += step;
                    }
                    self.wave -= 20;
                    if self.wave < 0 {
                        self.wave = 720;
                    }
                }
            }
            _ => {}
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_geometry() {
        let g = geo();
        let counts: Vec<usize> = g.rows.iter().map(|r| r.len()).collect();
        assert_eq!(counts, [13, 15, 15, 14, 14, 9]);
        assert_eq!((g.w, g.h, g.key_h), (600, 233, 27));
        assert_eq!((g.wave_x, g.mid_y, g.wave_r), (35, 146, 143));
        assert_eq!(g.bars.len(), 9);
        assert_eq!(g.rings.len(), 8);
        assert_eq!(g.strips.len(), 14);
    }

    #[test]
    fn fill_matches_driver() {
        assert_eq!(fill(50, 6), (3, 0));
        assert_eq!(fill(60, 6), (4, 10)); // 3 full rows and a dimmed fourth
        assert_eq!(fill(100, 6), (6, 0));
        assert_eq!(fill(0, 6), (0, 0));
    }

    #[test]
    fn levels_gain_and_smoothing() {
        let h = [[80; BANDS], [40; BANDS]];
        assert_eq!(levels(&h, DEFAULT_GAIN, 1)[0], 40);
        assert_eq!(levels(&h, DEFAULT_GAIN, 2)[0], 60);
        assert_eq!(levels(&h, 15, 1)[0], 80); // x2.0
        assert_eq!(levels(&h, 15, 2)[0], 90); // 100 (clamped) and 80
    }

    #[test]
    fn colorfull_cycle() {
        let mut c = Cycle::strips();
        assert_eq!(c.t[13], 120);
        assert_eq!(c.t[0], 1680);
        let mut t = [0; LED_COUNT];
        c.step(&mut t);
        let esc = KEYS[0].led as usize;
        assert_eq!(t[esc], lerp(0xff00, 200, 450, 0)); // first strip starts mid-fade from green
    }

    #[test]
    fn every_mode_draws() {
        let mut m = Music::default();
        let mut lv = [0; BANDS];
        for mode in 0..10u8 {
            let mut lit = false;
            for i in 0..60 {
                lv.iter_mut().for_each(|v| *v = if i % 6 < 3 { 90 } else { 30 });
                let f = m.step(mode, &lv, i % 2 == 0, 0xff00);
                lit |= f.iter().any(|c| c.iter().any(|&v| v > 0));
            }
            assert!(lit, "mode {mode} stayed dark");
        }
    }
}
