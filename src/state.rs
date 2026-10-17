//! Profiles in ~/.config/f75-studio/profiles/*.txt (simple key=value files) plus the name of the
//! active profile in state.txt. Each profile holds the full keyboard setup: lighting, sleep and the
//! base/Fn key layouts (missing layout = factory layout), text macros, palette and streamed effect.
use crate::protocol::{KEYMAP_LEN, LED_COUNT};
use std::path::PathBuf;

pub fn dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("f75-studio")
}

fn profiles_dir() -> PathBuf {
    dir().join("profiles")
}

#[derive(Clone)]
pub struct MacroDef {
    pub name: String,
    pub text: String,
    pub led: u8,
}

#[derive(Clone)]
pub struct Profile {
    pub name: String,
    pub mode: u8, // hardware effect id (protocol::EFFECTS)
    pub colours: [[u8; 3]; LED_COUNT],
    pub brush: [u8; 3],
    pub brightness: u8,
    pub speed: u8,
    pub colour: u8,
    pub sleep: u16,
    pub keymaps: [Option<Vec<u8>>; 2], // base, Fn
    pub macros: Vec<MacroDef>,
    pub palette: u8,
    pub fx: String, // streamed studio effect name, "" = none
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            mode: crate::protocol::CUSTOM_MODE,
            colours: [[0; 3]; LED_COUNT],
            brush: [57, 255, 106],
            brightness: 9,
            speed: 2,
            colour: 0,
            sleep: 300,
            keymaps: [None, None],
            macros: vec![],
            palette: 0,
            fx: String::new(),
        }
    }

    fn file(name: &str) -> PathBuf {
        let slug: String = name.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
        profiles_dir().join(format!("{slug}.txt"))
    }

    fn parse(text: &str) -> Self {
        let mut p = Profile::new("Default");
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "name" => p.name = v.into(),
                "mode" => p.mode = v.parse().unwrap_or(p.mode),
                "colours" => {
                    for (i, h) in v.split(',').enumerate().take(LED_COUNT) {
                        if let Some(c) = unhex3(h) {
                            p.colours[i] = c;
                        }
                    }
                }
                "brush" => p.brush = unhex3(v).unwrap_or(p.brush),
                "brightness" => p.brightness = v.parse().unwrap_or(9),
                "speed" => p.speed = v.parse().unwrap_or(2),
                "colour" => p.colour = v.parse().unwrap_or(0),
                "sleep" => p.sleep = v.parse().unwrap_or(300),
                "keymap0" | "keymap1" => {
                    let b = unhex(v);
                    if b.len() == KEYMAP_LEN {
                        p.keymaps[(k == "keymap1") as usize] = Some(b);
                    }
                }
                "palette" => p.palette = v.parse().unwrap_or(0),
                "fx" => p.fx = v.into(),
                "macro" => {
                    let mut f = v.splitn(3, '|');
                    if let (Some(led), Some(name), Some(text)) = (f.next(), f.next(), f.next()) {
                        let text = String::from_utf8(unhex(text)).unwrap_or_default();
                        if let Ok(led) = led.parse() {
                            p.macros.push(MacroDef { name: name.into(), text, led });
                        }
                    }
                }
                _ => {}
            }
        }
        p
    }

    pub fn save(&self) {
        let mut out = format!("name={}\nmode={}\n", self.name, self.mode);
        out += &format!("colours={}\n", self.colours.iter().map(|c| hex3(*c)).collect::<Vec<_>>().join(","));
        out += &format!(
            "brush={}\nbrightness={}\nspeed={}\ncolour={}\nsleep={}\n",
            hex3(self.brush),
            self.brightness,
            self.speed,
            self.colour,
            self.sleep
        );
        out += &format!("palette={}\nfx={}\n", self.palette, self.fx);
        for m in &self.macros {
            let name: String = m.name.chars().filter(|c| *c != '|' && *c != '\n').collect();
            out += &format!("macro={}|{}|{}\n", m.led, name, m.text.bytes().map(|b| format!("{b:02x}")).collect::<String>());
        }
        for (i, k) in self.keymaps.iter().enumerate() {
            if let Some(k) = k {
                out += &format!("keymap{i}={}\n", k.iter().map(|b| format!("{b:02x}")).collect::<String>());
            }
        }
        let _ = std::fs::create_dir_all(profiles_dir());
        let _ = std::fs::write(Self::file(&self.name), out);
        let _ = std::fs::write(dir().join("state.txt"), format!("profile={}\n", self.name));
    }

    pub fn delete(name: &str) {
        let _ = std::fs::remove_file(Self::file(name));
    }

    pub fn load(name: &str) -> Option<Self> {
        std::fs::read_to_string(Self::file(name)).ok().map(|t| Self::parse(&t))
    }

    /// All profile names (sorted), creating "Default" on first run (migrating the old state.txt).
    pub fn list() -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(profiles_dir())
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                    .map(|t| Self::parse(&t).name)
                    .collect()
            })
            .unwrap_or_default();
        if names.is_empty() {
            let old = std::fs::read_to_string(dir().join("state.txt")).unwrap_or_default();
            let mut p = Self::parse(&old);
            p.name = "Default".into();
            if !old.contains("colours=") {
                crate::ui::preset_rainbow(&mut p.colours);
            }
            p.save();
            names.push(p.name);
        }
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    pub fn active() -> Self {
        let names = Self::list();
        let want = std::fs::read_to_string(dir().join("state.txt"))
            .ok()
            .and_then(|t| t.lines().find_map(|l| l.strip_prefix("profile=").map(String::from)));
        want.and_then(|n| Self::load(&n)).or_else(|| Self::load(&names[0])).unwrap_or_else(|| Self::new("Default"))
    }
}

fn hex3(c: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}
fn unhex3(s: &str) -> Option<[u8; 3]> {
    let v = u32::from_str_radix(s.get(..6)?, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()).collect()
}

/// First good base/Fn layout read is kept as the factory backup for "Restore default layout".
/// A base-layer backup that fails the sanity check (e.g. an old shifted read) is replaced.
pub fn keymap_backup(layer: u8, current: &[u8]) -> Vec<u8> {
    let p = dir().join(format!("keymap-layer{layer}.bin"));
    let legacy = ["24g", "wired"].map(|l| dir().join(format!("keymap-{l}-layer{layer}.bin")));
    for f in std::iter::once(&p).chain(legacy.iter()) {
        if let Ok(b) = std::fs::read(f) {
            let sane = layer != 0 || crate::protocol::keymap_encoding_ok(&b);
            if b.len() == current.len() && sane {
                if f != &p {
                    let _ = std::fs::write(&p, &b);
                }
                return b;
            }
        }
    }
    if layer == 0 && !crate::protocol::keymap_encoding_ok(current) {
        return current.to_vec(); // never save a suspicious layout as the factory backup
    }
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(&p, current);
    current.to_vec()
}
