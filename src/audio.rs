//! System-audio capture (PipeWire / PulseAudio monitor of the default output) -> 21 spectrum bands.
use libpulse_binding::def::BufferAttr;
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const BANDS: usize = 21; // one per keyboard column
const RATE: u32 = 44_100;
const N: usize = 1024;

#[derive(Default)]
pub struct Levels {
    pub bands: [f32; BANDS], // 0..1, smoothed
    pub bass: f32,
    pub error: Option<String>,
    pub music: Vec<[i32; crate::aula_music::BANDS]>, // recent AULA band levels (before gain)
}

/// What the effects see each frame.
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub bands: [f32; BANDS],
    pub bass: f32,
}

impl Audio {
    /// The current levels, scaled by sensitivity.
    pub fn snapshot(&self, sensitivity: f32) -> Snapshot {
        let l = self.levels.lock().unwrap();
        let k = |v: f32| (v * sensitivity).min(1.0);
        Snapshot { bands: l.bands.map(k), bass: k(l.bass) }
    }

    /// AULA's band levels with its gain and smoothing applied.
    pub fn music(&self, gain: u8, smooth: u8) -> [i32; crate::aula_music::BANDS] {
        crate::aula_music::levels(&self.levels.lock().unwrap().music, gain, smooth)
    }
}

pub struct Audio {
    pub levels: Arc<Mutex<Levels>>,
    stop: Arc<AtomicBool>,
}

impl Audio {
    pub fn start() -> Self {
        let levels = Arc::new(Mutex::new(Levels::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (l, s) = (levels.clone(), stop.clone());
        std::thread::spawn(move || {
            if let Err(e) = run(&l, &s) {
                l.lock().unwrap().error = Some(e);
            }
        });
        Self { levels, stop }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run(levels: &Mutex<Levels>, stop: &AtomicBool) -> Result<(), String> {
    let spec = Spec { format: Format::F32le, channels: 2, rate: RATE };
    let attr = BufferAttr { maxlength: u32::MAX, tlength: u32::MAX, prebuf: u32::MAX, minreq: u32::MAX, fragsize: (N * 4) as u32 };
    let pa = Simple::new(None, "F75 Studio", Direction::Record, Some("@DEFAULT_MONITOR@"), "audio sync", &spec, None, Some(&attr))
        .map_err(|e| format!("audio capture failed: {e} (is PipeWire/PulseAudio running?)"))?;
    let fft = FftPlanner::<f32>::new().plan_fft_forward(N);
    let mut raw = vec![0u8; N * 8];
    let window: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
    // log-spaced band edges 40 Hz .. 12 kHz
    let edges: Vec<usize> = (0..=BANDS)
        .map(|i| {
            let f = 40.0 * (12_000.0f32 / 40.0).powf(i as f32 / BANDS as f32);
            ((f / RATE as f32 * N as f32) as usize).clamp(1, N / 2)
        })
        .collect();
    let mut peak = 1e-3f32;
    while !stop.load(Ordering::Relaxed) {
        pa.read(&mut raw).map_err(|e| format!("audio read failed: {e}"))?;
        let f = |b: &[u8]| f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let mut left: Vec<Complex<f32>> = raw.chunks_exact(8).map(|b| Complex::new(f(&b[0..4]), 0.0)).collect();
        let mut buf: Vec<Complex<f32>> =
            raw.chunks_exact(8).zip(&window).map(|(b, w)| Complex::new((f(&b[0..4]) + f(&b[4..8])) * 0.5 * w, 0.0)).collect();
        fft.process(&mut buf);
        // AULA's levels: left channel, no window, one bin per band
        fft.process(&mut left);
        let music = crate::aula_music::BAND_HZ.map(|hz| {
            let bin = (N as f64 * hz / RATE as f64) as usize;
            if hz > RATE as f64 / 2.0 { 0 } else { crate::aula_music::raw_level(left[bin].norm_sqr()) }
        });
        let mut bands = [0f32; BANDS];
        for (i, b) in bands.iter_mut().enumerate() {
            let (a, z) = (edges[i], edges[i + 1].max(edges[i] + 1));
            *b = buf[a..z].iter().map(|c| c.norm()).fold(0.0, f32::max);
        }
        let frame_peak = bands.iter().cloned().fold(0.0, f32::max);
        peak = (peak * 0.995).max(frame_peak).max(1e-3); // auto gain
        let mut l = levels.lock().unwrap();
        for (o, b) in l.bands.iter_mut().zip(bands) {
            let v = ((b / peak).powf(0.6) * 1.15).min(1.0);
            *o = if v > *o { v } else { *o * 0.82 + v * 0.18 }; // fast attack, slow release
        }
        l.bass = (l.bands[0] + l.bands[1] + l.bands[2] + l.bands[3]) / 4.0;
        l.music.push(music);
        if l.music.len() > 16 {
            l.music.remove(0);
        }
        drop(l);
    }
    Ok(())
}
