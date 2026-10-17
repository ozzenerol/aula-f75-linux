//! On-keyboard macros, from OemDrv `FUN_0048d2d0` (buffer) and `FUN_0048cf00` (events).
//!
//! Buffer: N × [addr u16, size u16] (addr from buffer start), then per macro
//! `name_len, UTF-16LE name (≤ 30 chars), events…`; only the used bytes are written.
//! Event: `[(type << 4) | release << 7 | delay[19:16], delay[15:8], delay[7:0], code]`
//! type 0 = key (HID usage), 1 = modifier (0xe0-0xe7), 2 = mouse button, 3/4 = mouse X/Y, 5 = wheel.
//! Key matrix code for a macro key: `[0x03, mode, count, macro_index]` (mode 1 = play `count` times).

pub struct Macro {
    pub name: String,
    pub events: Vec<Ev>,
}

#[derive(Clone, Copy)]
pub struct Ev {
    pub usage: u8,
    pub down: bool,
    pub delay_ms: u32, // wait after this event
}

pub fn key_code(index: u8, count: u8) -> [u8; 4] {
    [3, 1, count.max(1), index]
}

fn is_modifier(usage: u8) -> bool {
    (0xe0..=0xe7).contains(&usage)
}

/// Text to key events (US layout); unsupported characters are skipped.
pub fn text_events(text: &str, delay: u32) -> Vec<Ev> {
    let mut ev = vec![];
    for ch in text.chars() {
        let Some((usage, shift)) = char_usage(ch) else { continue };
        if shift {
            ev.push(Ev { usage: 0xe1, down: true, delay_ms: 4 });
        }
        ev.push(Ev { usage, down: true, delay_ms: delay });
        ev.push(Ev { usage, down: false, delay_ms: 4 });
        if shift {
            ev.push(Ev { usage: 0xe1, down: false, delay_ms: delay });
        }
    }
    ev
}

fn char_usage(ch: char) -> Option<(u8, bool)> {
    const PLAIN: &str = "1234567890\n\t -=[]\\;'`,./";
    const PLAIN_CODES: [u8; 23] =
        [0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x33, 0x34, 0x35, 0x36, 0x37];
    const SHIFTED: &str = "!@#$%^&*()";
    const SHIFTED2: &str = "_+{}|:\"~<>?";
    const SHIFTED2_CODES: [u8; 11] = [0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38];
    if ch.is_ascii_lowercase() {
        return Some((0x04 + (ch as u8 - b'a'), false));
    }
    if ch.is_ascii_uppercase() {
        return Some((0x04 + (ch as u8 - b'A'), true));
    }
    if ch == '/' {
        return Some((0x38, false));
    }
    if let Some(i) = PLAIN.chars().position(|c| c == ch) {
        return PLAIN_CODES.get(i).map(|u| (*u, false));
    }
    if let Some(i) = SHIFTED.chars().position(|c| c == ch) {
        return Some((PLAIN_CODES[i], true));
    }
    SHIFTED2.chars().position(|c| c == ch).map(|i| (SHIFTED2_CODES[i], true))
}

fn blob(m: &Macro) -> Vec<u8> {
    let name: Vec<u8> = m.name.encode_utf16().take(30).flat_map(|u| u.to_le_bytes()).collect();
    let mut b = vec![name.len() as u8];
    b.extend(name);
    for e in &m.events {
        let d = e.delay_ms.min(0xfffff);
        let kind: u8 = if is_modifier(e.usage) { 1 } else { 0 };
        let mut head = (kind << 4) | ((d >> 16) as u8 & 0x0f);
        if !e.down {
            head |= 0x80;
        }
        b.extend([head, (d >> 8) as u8, d as u8, e.usage]);
    }
    b
}

/// The bytes OemDrv sends: address table + macro blobs, no padding.
pub fn buffer(macros: &[Macro]) -> Vec<u8> {
    let blobs: Vec<Vec<u8>> = macros.iter().map(blob).collect();
    let mut out = vec![];
    let mut addr = macros.len() * 4;
    for b in &blobs {
        out.extend((addr as u16).to_le_bytes());
        out.extend((b.len() as u16).to_le_bytes());
        addr += b.len();
    }
    for b in blobs {
        out.extend(b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_events_use_type_0_modifiers_type_1() {
        let b = buffer(&[Macro { name: "hi".into(), events: text_events("Hi", 20) }]);
        // table: addr 4, size = 1 + 4 (name) + 6 events * 4
        assert_eq!(&b[..4], &[4, 0, 29, 0]);
        assert_eq!(&b[4..9], &[4, b'h', 0, b'i', 0]);
        assert_eq!(&b[9..13], &[0x10, 0, 4, 0xe1]); // Shift down: modifier type 1
        assert_eq!(&b[13..17], &[0x00, 0, 20, 0x0b]); // H down: key type 0
        assert_eq!(&b[17..21], &[0x80, 0, 4, 0x0b]); // H up: bit 7
        assert_eq!(&b[21..25], &[0x90, 0, 20, 0xe1]); // Shift up
        assert_eq!(&b[25..29], &[0x00, 0, 20, 0x0c]); // i down
        assert_eq!(b.len(), 33);
    }

    #[test]
    fn two_macros_addresses() {
        let m = |s: &str| Macro { name: "m".into(), events: text_events(s, 10) };
        let b = buffer(&[m("a"), m("bb")]);
        let (a0, s0) = (u16::from_le_bytes([b[0], b[1]]) as usize, u16::from_le_bytes([b[2], b[3]]) as usize);
        let (a1, s1) = (u16::from_le_bytes([b[4], b[5]]) as usize, u16::from_le_bytes([b[6], b[7]]) as usize);
        assert_eq!((a0, a1), (8, 8 + s0));
        assert_eq!(a1 + s1, b.len());
    }
}
