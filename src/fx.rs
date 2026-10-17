//! Software effects streamed as live frames: studio effects and audio-reactive effects, each drawn
//! with a colour palette.
use crate::audio::{Snapshot, BANDS};
use crate::layout::KEYS;
use crate::protocol::LED_COUNT;

pub type Colours = [[u8; 3]; LED_COUNT];

pub const PALETTES: &[(&str, &[[u8; 3]])] = &[
    ("Neon", &[[0, 255, 90], [190, 255, 0], [255, 40, 0]]),
    ("Rainbow", &[[255, 0, 0], [255, 160, 0], [255, 255, 0], [0, 255, 0], [0, 200, 255], [60, 0, 255], [220, 0, 255]]),
    ("Synthwave", &[[40, 10, 255], [255, 0, 210], [0, 230, 255]]),
    ("Ocean", &[[0, 20, 140], [0, 140, 255], [140, 255, 255]]),
    ("Fire", &[[140, 0, 0], [255, 60, 0], [255, 220, 40]]),
    ("Sunset", &[[255, 50, 90], [255, 150, 40], [120, 40, 255]]),
    ("Ice", &[[220, 245, 255], [80, 170, 255], [20, 40, 210]]),
    ("Toxic", &[[10, 255, 30], [170, 255, 0], [0, 110, 40]]),
];

pub fn sample(pal: &[[u8; 3]], t: f32) -> [u8; 3] {
    let t = t.rem_euclid(1.0) * (pal.len() - 1) as f32;
    let i = (t as usize).min(pal.len() - 2);
    let f = t - i as f32;
    let (a, b) = (pal[i], pal[i + 1]);
    [0, 1, 2].map(|c| (a[c] as f32 + (b[c] as f32 - a[c] as f32) * f) as u8)
}

fn scale(c: [u8; 3], v: f32) -> [u8; 3] {
    let v = v.clamp(0.0, 1.0);
    c.map(|x| (x as f32 * v) as u8)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fx {
    Wave,
    Breathe,
    Twinkle,
    Plasma,
    Rain,
    Fire,
    Spectrum,
    Mirror,
    /// One of AULA's own music modes (0..9), see aula_music.
    Aula(u8),
}

pub const STUDIO: &[(Fx, &str)] =
    &[(Fx::Wave, "Wave"), (Fx::Breathe, "Breathe"), (Fx::Twinkle, "Twinkle"), (Fx::Plasma, "Plasma"), (Fx::Rain, "Matrix Rain"), (Fx::Fire, "Fire")];
use crate::aula_music::NAMES as M;
pub const AUDIO: &[(Fx, &str)] = &[
    (Fx::Spectrum, "Spectrum"),
    (Fx::Mirror, "Mirror"),
    (Fx::Aula(0), M[0]),
    (Fx::Aula(1), M[1]),
    (Fx::Aula(2), M[2]),
    (Fx::Aula(3), M[3]),
    (Fx::Aula(4), M[4]),
    (Fx::Aula(5), M[5]),
    (Fx::Aula(6), M[6]),
    (Fx::Aula(7), M[7]),
    (Fx::Aula(8), M[8]),
    (Fx::Aula(9), M[9]),
];

/// Physical key geometry: led -> (x 0..1 left→right, y 0..1 top→bottom, column 0..20, row 0..5).
/// Key centres span x 42.5-617.5 and y 44-250 on the layout canvas (6 rows).
fn geo() -> impl Iterator<Item = (usize, f32, f32, usize, usize)> {
    KEYS.iter().map(|k| {
        let x = (((k.rect[0] + k.rect[2]) / 2.0 - 42.5) / 575.0).clamp(0.0, 1.0);
        let y = (((k.rect[1] + k.rect[3]) / 2.0 - 44.0) / 206.0).clamp(0.0, 1.0);
        (k.led as usize, x, y, (x * 20.0).round() as usize, (y * 5.0).round() as usize)
    })
}

pub struct Engine {
    seed: u32,
    glow: [f32; LED_COUNT],
    hue: [f32; LED_COUNT],
    drops: [f32; 21],
    heat: [[f32; 6]; 21],
    last_t: f32,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            seed: 0x1234_5678,
            glow: [0.0; LED_COUNT],
            hue: [0.0; LED_COUNT],
            drops: [0.0; 21],
            heat: [[0.0; 6]; 21],
            last_t: 0.0,
        }
    }
}

impl Engine {
    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 10_000) as f32 / 10_000.0
    }

    pub fn frame(&mut self, fx: Fx, pal: &[[u8; 3]], t: f32, speed: f32, a: &Snapshot) -> Colours {
        let dt = (t - self.last_t).clamp(0.0, 0.2);
        self.last_t = t;
        let ts = t * speed;
        let mut c = [[0u8; 3]; LED_COUNT];
        match fx {
            Fx::Wave => geo().for_each(|(l, x, _, _, _)| c[l] = sample(pal, x - ts * 0.25)),
            Fx::Breathe => {
                let v = (0.5 + 0.5 * (ts * 2.0).sin()).powi(2) * 0.95 + 0.05;
                geo().for_each(|(l, x, _, _, _)| c[l] = scale(sample(pal, ts * 0.05 + x * 0.3), v));
            }
            Fx::Twinkle => {
                for (l, ..) in geo().collect::<Vec<_>>() {
                    self.glow[l] = (self.glow[l] - dt * 1.2 * speed).max(0.0);
                    if self.rand() < dt * 0.9 * speed {
                        self.glow[l] = 1.0;
                        self.hue[l] = self.rand();
                    }
                    c[l] = scale(sample(pal, self.hue[l]), self.glow[l]);
                }
            }
            Fx::Plasma => geo().for_each(|(l, x, y, _, _)| {
                let v = (x * 7.0 + ts).sin() + (y * 5.0 - ts * 1.3).sin() + ((x + y) * 4.0 + ts * 0.7).sin();
                c[l] = sample(pal, v / 6.0 + 0.5);
            }),
            Fx::Rain => {
                for col in 0..21 {
                    self.drops[col] += dt * speed * (3.0 + (col as f32 * 1.7).sin().abs() * 3.0);
                    if self.drops[col] > 9.0 + self.rand() * 4.0 {
                        self.drops[col] = 0.0;
                    }
                }
                geo().for_each(|(l, _, _, col, row)| {
                    let d = self.drops[col] - row as f32;
                    let v = if (0.0..4.0).contains(&d) { 1.0 - d / 4.0 } else { 0.0 };
                    c[l] = scale(sample(pal, 1.0 - v * 0.6), v);
                });
            }
            Fx::Fire => {
                let energy = 0.6 + a.bass * 0.8;
                for col in 0..21 {
                    for row in 0..6 {
                        let base = if row == 5 { energy * (0.6 + self.rand() * 0.5) } else { self.heat[col][row + 1] * (0.55 + self.rand() * 0.35) };
                        self.heat[col][row] = self.heat[col][row] * 0.35 + base * 0.65;
                    }
                }
                geo().for_each(|(l, _, _, col, row)| {
                    let h = self.heat[col][row].min(1.0);
                    c[l] = scale(sample(pal, h * 0.99), h * 1.4);
                });
            }
            Fx::Spectrum => geo().for_each(|(l, _, _, col, row)| {
                let lit = (a.bands[col.min(BANDS - 1)] * 6.0).round() as usize;
                let h = 5 - row;
                if h < lit {
                    c[l] = sample(pal, h as f32 / 5.0 * 0.99);
                }
            }),
            Fx::Mirror => geo().for_each(|(l, _, _, col, row)| {
                let band = (col as i32 - 10).unsigned_abs() as usize * 2; // bass in the middle
                let v = a.bands[band.min(BANDS - 1)];
                let half = (v * 3.5).round() as i32;
                let d = (row as i32 * 2 - 5).abs() / 2; // distance from middle rows
                if d < half {
                    c[l] = sample(pal, (col as f32 - 10.0).abs() / 10.0 * 0.99);
                }
            }),
            Fx::Aula(_) => {} // drawn by aula_music

        }
        c
    }
}
