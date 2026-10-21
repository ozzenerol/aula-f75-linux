//! `f75-studio doctor`: tests lighting + remapping on the active link, asks the user what the
//! keyboard shows, restores everything, and writes ~/f75-doctor-<link>.txt with a raw packet trace.
use crate::device::{hex, F75};
use crate::layout::KEYS;
use crate::macros;
use crate::protocol::{self as p, Link, LED_COUNT};
use anyhow::Result;
use std::io::{BufRead, Write};
use std::time::Instant;

struct Report {
    lines: Vec<String>,
    pass: usize,
    fail: usize,
}

impl Report {
    fn say(&mut self, s: impl Into<String>) {
        let s = s.into();
        println!("{s}");
        self.lines.push(s);
    }
    fn result(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        let d = detail.into();
        if ok { self.pass += 1 } else { self.fail += 1 }
        self.say(format!("[{}] {name}{}", if ok { "PASS" } else { "FAIL" }, if d.is_empty() { String::new() } else { format!(" - {d}") }));
    }
}

/// On 2.4G/Bluetooth, key releases can be lost while we flood the radio with commands, so Linux
/// auto-repeats the last key (usually Enter). Before every prompt: drop queued input; after the
/// answer: pause so the Enter release reaches the computer before the next burst of commands.
fn ask_raw(q: &str, accept: impl Fn(&str) -> bool) -> String {
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(300));
        unsafe {
            libc::tcflush(0, libc::TCIFLUSH);
        }
        print!("    ? {q} ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
            return String::new();
        }
        let a = line.trim().to_lowercase();
        if accept(&a) {
            std::thread::sleep(std::time::Duration::from_millis(800));
            return a;
        }
        println!("      (didn't catch that - if the screen is scrolling, tap any key once to stop it)");
    }
    String::new()
}

fn ask(q: &str) -> String {
    ask_raw(q, |a| !a.is_empty())
}

fn yes(q: &str) -> bool {
    ask_raw(&format!("{q} [y/n]"), |a| a.starts_with('y') || a.starts_with('n')).starts_with('y')
}

fn step<T>(r: &mut Report, kb: &mut F75, name: &str, f: impl FnOnce(&mut F75) -> Result<T>) -> Option<T> {
    kb.log.clear();
    let t0 = Instant::now();
    match f(kb) {
        Ok(v) => {
            let ms = t0.elapsed().as_millis();
            let logs = kb.log.join("; ");
            r.say(format!("    {name}: sent in {ms} ms {logs}"));
            Some(v)
        }
        Err(e) => {
            r.result(name, false, format!("{e:#}"));
            None
        }
    }
}

pub fn run(dry: Option<Link>) -> Result<()> {
    let mut r = Report { lines: vec![], pass: 0, fail: 0 };
    r.say(format!("f75-studio doctor {} · {}", env!("CARGO_PKG_VERSION"), chrono_now()));
    let present = F75::detect();
    r.say(format!("detected: {}", if present.is_empty() { "nothing".into() } else {
        present.iter().map(|(l, p)| format!("{} ({p})", l.name())).collect::<Vec<_>>().join(", ")
    }));
    let opened = match dry { Some(l) => Ok(F75::dry_run(l)), None => F75::open() };
    let mut kb = match opened {
        Ok(k) => k,
        Err(e) => {
            r.result("open keyboard", false, format!("{e:#}"));
            return finish(r, "none", None);
        }
    };
    let link = kb.link;
    kb.trace = Some(vec![]);
    r.result("open keyboard", true, format!("{} via {}", link.name(), kb.path));
    if link == Link::Receiver {
        match kb.online() {
            Ok(o) => r.result("keyboard linked to receiver", o, if o { "" } else { "press a key to wake it, then rerun" }),
            Err(e) => r.result("keyboard linked to receiver", false, format!("{e:#}")),
        }
    }

    r.say("\nAnswer questions with ANY keyboard (e.g. the laptop's) - the F75 may stop typing during tests.");
    let typed = ask("F75 typing check: type  qwe  ON THE F75, then Enter:");
    r.result("F75 types before tests", typed.contains("qwe"), format!("typed {typed:?}"));

    // ---------------------------------------------------------------- lighting
    r.say("\n== LIGHTING");
    let original = step(&mut r, &mut kb, "read LED block", |k| k.get_led_block());
    // keep the keyboard awake for the whole test (60 s idle sleep swallows the wake-up key on wireless)
    let original_sleep = original.as_ref().map(|b| b[p::SLEEP_OFFSET]);
    if link.wireless() {
        step(&mut r, &mut kb, "disable sleep during test", |k| k.set_sleep(0));
    }
    let readable = kb.log.iter().all(|l| !l.contains("using")) && original.is_some();
    r.result("LED block read from keyboard", readable, if readable { "" } else { "fell back to a saved copy" });

    if step(&mut r, &mut kb, "set Rainbow", |k| k.set_effect(3, 9, 2, 0)).is_some() {
        r.result("effect: Rainbow", yes("Is the keyboard showing a moving RAINBOW effect?"), "");
    }
    if step(&mut r, &mut kb, "set Fixed On red", |k| k.set_effect(1, 9, 0, 0)).is_some() {
        r.result("effect: Fixed On red", yes("Is the whole keyboard SOLID RED now?"), "");
        if let Some(b) = step(&mut r, &mut kb, "read back", |k| k.get_led_block()) {
            r.result("read-back matches (mode 1)", b[10] == 1, format!("byte[10]={} byte[0x3a..0x3c]={}", b[10], hex(&b[0x3a..0x3c])));
        }
    }
    let mut wasd = [[0u8; 3]; LED_COUNT];
    for k in KEYS.iter().filter(|k| [0x57, 0x41, 0x53, 0x44].contains(&k.vk)) {
        wasd[k.led as usize] = [0, 255, 0];
    }
    if step(&mut r, &mut kb, "per-key WASD green", |k| k.set_per_key(&wasd, 9)).is_some() {
        r.result("per-key colours", yes("Are ONLY W, A, S, D lit GREEN (everything else off)?"), "");
    }
    if step(&mut r, &mut kb, "per-key all white, brightness 3", |k| k.set_per_key(&[[255, 255, 255]; LED_COUNT], 3)).is_some() {
        r.result("brightness", yes("Is the whole keyboard DIM WHITE?"), "");
    }
    let typed = ask("F75 typing check: type  qwe  ON THE F75, then Enter:");
    r.result("F75 types after lighting writes", typed.contains("qwe"), format!("typed {typed:?}"));
    r.say("    streaming a live rainbow for 6 seconds…");
    let t0 = Instant::now();
    let mut frames = 0;
    let mut live_err = None;
    while t0.elapsed().as_secs_f32() < 6.0 {
        let t = t0.elapsed().as_secs_f32();
        let mut c = [[0u8; 3]; LED_COUNT];
        for k in KEYS {
            c[k.led as usize] = hue(((k.rect[0] / 679.0) * 1.4 - t * 0.5).rem_euclid(1.0));
        }
        if let Err(e) = kb.realtime(&c) {
            live_err = Some(format!("{e:#}"));
            break;
        }
        frames += 1;
    }
    match live_err {
        Some(e) => r.result("live frames", false, e),
        None => {
            let fps = frames as f32 / 6.0;
            r.result("live frames (Wave FX / audio sync)", yes(&format!("Did you see a MOVING rainbow just now? ({fps:.1} fps)")), format!("{fps:.1} fps"));
        }
    }

    // ---------------------------------------------------------------- keymap
    r.say("\n== KEY REMAP");
    if let Some(km) = step(&mut r, &mut kb, "read base layer", |k| k.get_keymap(0)) {
        let ok = p::keymap_encoding_ok(&km);
        r.result("base layer looks right", ok, "");
        if ok {
            let caps = KEYS.iter().find(|k| k.vk == 0x14).unwrap().led as usize;
            let mut m = km.clone();
            m[caps * 4..caps * 4 + 4].copy_from_slice(&p::key_code(0x1d, 0)); // Caps Lock -> Z
            if step(&mut r, &mut kb, "remap Caps Lock to Z", |k| k.set_keymap(0, &m)).is_some() {
                if let Some(b) = step(&mut r, &mut kb, "read back", |k| k.get_keymap(0)) {
                    r.result("remap read-back", b[caps * 4..caps * 4 + 4] == m[caps * 4..caps * 4 + 4], hex(&b[caps * 4..caps * 4 + 4]));
                }
                let typed = ask_raw("On the F75 press CAPS LOCK three times, then Enter:", |_| true);
                r.result("remap works (Caps Lock types z)", typed.contains('z'), format!("typed {typed:?}"));
                let restore = p::factory_layer0();
                let base = if p::keymap_encoding_ok(&km) { km.clone() } else { restore };
                if step(&mut r, &mut kb, "restore layout", |k| k.set_keymap(0, &base)).is_some() {
                    let typed = ask_raw("On the F75 press CAPS LOCK twice, then Enter (should type nothing):", |_| true);
                    r.result("layout restored", !typed.contains('z'), format!("typed {typed:?}"));
                }
                // macro: F12 types "hi" (format from OemDrv FUN_0048d2d0 / FUN_0048cf00)
                r.say("\n== MACRO");
                let f12 = KEYS.iter().find(|k| k.vk == 0x7b).unwrap().led as usize;
                let mut m = base.clone();
                m[f12 * 4..f12 * 4 + 4].copy_from_slice(&macros::key_code(0, 1));
                let buf = macros::buffer(&[macros::Macro { name: "doctor".into(), events: macros::text_events("hi", 12) }]);
                r.say(format!("    macro bytes: {}", hex(&buf)));
                if link != Link::Wired {
                    r.say("    skipped: macros need the USB-C cable");
                } else if step(&mut r, &mut kb, "assign macro to F12", |k| k.set_keymap(0, &m)).is_some()
                    && step(&mut r, &mut kb, "write macro area", |k| k.set_macros(&buf)).is_some()
                {
                    if let Some(b) = step(&mut r, &mut kb, "read macro area back", |k| k.get_macros(buf.len())) {
                        r.result("macro read-back", b.get(..buf.len()) == Some(&buf[..]), hex(&b[..b.len().min(48)]));
                    }
                    let typed = ask_raw("On the F75 press F12 once, then Enter:", |_| true);
                    r.result("macro works (F12 types hi)", typed.contains("hi"), format!("typed {typed:?}"));
                    step(&mut r, &mut kb, "remove macro", |k| k.set_keymap(0, &base));
                }
            }
        }
    }

    // ---------------------------------------------------------------- restore lighting
    r.say("\n== RESTORE");
    if let Some(b) = original {
        let led = kb.cmds.led;
        step(&mut r, &mut kb, "restore lighting", |k| k.write(led, 0, &b, 15));
        if let Some(sl) = original_sleep {
            r.say(format!("    (sleep restored to hardware value {sl}; set Never in the app's Power tab for always-on)"));
        }
    }
    let typed = ask("Final F75 typing check: type  qwe  ON THE F75, then Enter:");
    r.result("F75 types after the test", typed.contains("qwe"), format!("typed {typed:?}"));
    let trace = kb.trace.take();
    finish(r, &link.name().to_lowercase().replace('.', ""), trace)
}

fn finish(mut r: Report, link: &str, trace: Option<Vec<String>>) -> Result<()> {
    r.say(format!("\nRESULT: {} passed, {} failed", r.pass, r.fail));
    let mut text = r.lines.join("\n");
    if let Some(t) = trace {
        text += "\n\n== RAW PACKETS (first 600)\n";
        text += &t.join("\n");
    }
    let path = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(format!("f75-doctor-{link}.txt"));
    std::fs::write(&path, text)?;
    println!("report saved: {}", path.display());
    Ok(())
}

fn chrono_now() -> String {
    std::process::Command::new("date").arg("-Is").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn hue(h: f32) -> [u8; 3] {
    let f = |n: f32| {
        let k = (n + h * 6.0) % 6.0;
        ((1.0 - (k.min(4.0 - k).clamp(0.0, 1.0))) * 255.0) as u8
    };
    [f(5.0), f(3.0), f(1.0)]
}
