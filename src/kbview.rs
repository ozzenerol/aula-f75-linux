//! Keyboard renderer: the F75 (black edition) drawn as simple 3D geometry and projected with an
//! orthographic camera. Pitch 0 is the flat top view; the 3D view tilts and turns the same model.
//! Units are the layout canvas (1u key pitch ~38.5, ~2 units per mm).
use crate::layout::{Key, KEYS};
use eframe::egui::{self, pos2, vec2, Color32, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke, TextureId};

type V3 = [f32; 3];

#[derive(Clone, Copy, PartialEq)]
pub struct Cam {
    pub yaw: f32,
    pub pitch: f32,
}

impl Cam {
    pub const TOP: Cam = Cam { yaw: 0.0, pitch: 0.0 };
    pub const ISO: Cam = Cam { yaw: -0.30, pitch: 0.80 };

    pub fn toward(self, to: Cam, k: f32) -> Cam {
        Cam { yaw: self.yaw + (to.yaw - self.yaw) * k, pitch: self.pitch + (to.pitch - self.pitch) * k }
    }

    pub fn near(self, to: Cam) -> bool {
        (self.yaw - to.yaw).abs() < 0.002 && (self.pitch - to.pitch).abs() < 0.002
    }
}

// ---------------------------------------------------------------- model

const CASE: [f32; 4] = [6.0, 4.0, 656.0, 289.0];
const TRAY: [f32; 4] = [21.0, 21.0, 641.0, 272.0];
const CENTRE: [f32; 2] = [331.0, 146.5];
const KNOB: [f32; 2] = [617.0, 43.0];
const KNOB_R: f32 = 19.0;
const CAP_H: f32 = 15.0;

/// Top of the case: about 12 mm at the front and 30 mm at the back.
fn case_z(y: f32) -> f32 {
    let t = ((y - CASE[1]) / (CASE[3] - CASE[1])).clamp(0.0, 1.0);
    60.0 - 36.0 * t
}

fn plate_z(y: f32) -> f32 {
    case_z(y) - 8.0
}

const CHASSIS: Color32 = Color32::from_rgb(0x15, 0x15, 0x18);
const PLATE: Color32 = Color32::from_rgb(0x09, 0x09, 0x0b);
const BLUE: Color32 = Color32::from_rgb(0x34, 0x38, 0x50);
const BLACK: Color32 = Color32::from_rgb(0x1d, 0x1e, 0x24);
const YELLOW: Color32 = Color32::from_rgb(0xdc, 0xa6, 0x40);
const LEGEND: Color32 = Color32::from_rgb(0xf2, 0xc2, 0x7a);
const LEGEND_DARK: Color32 = Color32::from_rgb(0x1a, 0x16, 0x10);
const SILVER: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x92);
const KNOB_CAP: Color32 = Color32::from_rgb(0x15, 0x16, 0x1a);

fn cap_colour(k: &Key) -> Color32 {
    match k.vk {
        0x1B | 0x0D | 0x20 | 0x25..=0x28 => YELLOW,
        0x74..=0x77 => BLACK, // F5-F8
        0x08 | 0x2E | 0x21 | 0x22 | 0x23 | 0x09 | 0x14 | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0x5B | 0xA4 | 0xFA => BLACK,
        _ => BLUE,
    }
}

/// Footprint of a keycap on the plate, from the vendor layout rects (which are the cap tops).
fn footprint(k: &Key) -> [f32; 4] {
    let [x0, y0, x1, y1] = k.rect;
    let cy = (y0 + y1) / 2.0;
    [x0 - 5.0, cy - 18.0, x1 + 5.0, cy + 18.0]
}

/// Cherry-style taper: the top face is inset more at the front than at the back.
fn top_face(f: [f32; 4]) -> [f32; 4] {
    [f[0] + 3.4, f[1] + 2.0, f[2] - 3.4, f[3] - 4.8]
}

fn rounded(r: [f32; 4], rad: f32) -> Vec<[f32; 2]> {
    let rad = rad.min((r[2] - r[0]) / 2.0).min((r[3] - r[1]) / 2.0);
    let corners = [
        ([r[2] - rad, r[1] + rad], -90.0f32),
        ([r[2] - rad, r[3] - rad], 0.0),
        ([r[0] + rad, r[3] - rad], 90.0),
        ([r[0] + rad, r[1] + rad], 180.0),
    ];
    let mut pts = Vec::with_capacity(20);
    for (c, a0) in corners {
        for i in 0..=4 {
            let a = (a0 + 22.5 * i as f32).to_radians();
            pts.push([c[0] + rad * a.cos(), c[1] + rad * a.sin()]);
        }
    }
    pts
}

fn circle(c: [f32; 2], r: f32, n: usize) -> Vec<[f32; 2]> {
    (0..n).map(|i| {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        [c[0] + r * a.cos(), c[1] + r * a.sin()]
    }).collect()
}

/// Outward edge normals (clockwise outline in y-down coordinates).
fn normals(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    (0..pts.len()).map(|i| {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt().max(1e-6);
        [dy / l, -dx / l]
    }).collect()
}

// ---------------------------------------------------------------- camera

pub struct View {
    cam: Cam,
    centre: Pos2,
    scale: f32,
    look: V3,
}

impl View {
    fn raw(cam: Cam, v: V3) -> (f32, f32) {
        let (x, y) = (v[0] - CENTRE[0], v[1] - CENTRE[1]);
        let (sy, cy) = cam.yaw.sin_cos();
        let (sp, cp) = cam.pitch.sin_cos();
        let x1 = x * cy - y * sy;
        let y1 = x * sy + y * cy;
        (x1, y1 * cp - v[2] * sp)
    }

    /// Fits the whole keyboard into `rect`, then applies `zoom` and a screen `pan`.
    pub fn fit(rect: Rect, cam: Cam, zoom: f32, pan: egui::Vec2) -> View {
        let mut b = Rect::NOTHING;
        for &x in &[CASE[0], CASE[2]] {
            for &y in &[CASE[1], CASE[3]] {
                for &z in &[0.0, case_z(y) + CAP_H + 6.0] {
                    let (sx, sy) = Self::raw(cam, [x, y, z]);
                    b.extend_with(pos2(sx, sy));
                }
            }
        }
        let pad = 0.97 - 0.08 * (cam.pitch / Cam::ISO.pitch).clamp(0.0, 1.0); // room for the floor glow in 3D
        let scale = (rect.width() / b.width()).min(rect.height() / b.height()) * pad * zoom;
        let centre = rect.center() - b.center().to_vec2() * scale + pan;
        let (sy, cy) = cam.yaw.sin_cos();
        let (sp, cp) = cam.pitch.sin_cos();
        View { cam, centre, scale, look: [sp * sy, sp * cy, cp] }
    }

    /// Aspect ratio (width / height) of the keyboard as seen by `cam`.
    pub fn aspect(cam: Cam) -> f32 {
        let mut b = Rect::NOTHING;
        for &x in &[CASE[0], CASE[2]] {
            for &y in &[CASE[1], CASE[3]] {
                for &z in &[0.0, case_z(y) + CAP_H + 6.0] {
                    let (sx, sy) = Self::raw(cam, [x, y, z]);
                    b.extend_with(pos2(sx, sy));
                }
            }
        }
        b.width() / b.height()
    }

    pub fn p(&self, v: V3) -> Pos2 {
        let (x, y) = Self::raw(self.cam, v);
        self.centre + vec2(x, y) * self.scale
    }

    fn depth(&self, x: f32, y: f32) -> f32 {
        let (sy, cy) = self.cam.yaw.sin_cos();
        (x - CENTRE[0]) * sy + (y - CENTRE[1]) * cy
    }

    fn faces(&self, n: [f32; 2], nz: f32) -> f32 {
        n[0] * self.look[0] + n[1] * self.look[1] + nz * self.look[2]
    }
}

// ---------------------------------------------------------------- painting helpers

fn mul(c: Color32, f: f32) -> Color32 {
    let f = f.max(0.0);
    Color32::from_rgb((c.r() as f32 * f).min(255.0) as u8, (c.g() as f32 * f).min(255.0) as u8, (c.b() as f32 * f).min(255.0) as u8)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

fn glow(c: [u8; 3], a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn quad(m: &mut Mesh, p: [Pos2; 4], c: [Color32; 4]) {
    let i = m.vertices.len() as u32;
    for k in 0..4 {
        m.colored_vertex(p[k], c[k]);
    }
    m.add_triangle(i, i + 1, i + 2);
    m.add_triangle(i, i + 2, i + 3);
}

/// Filled convex polygon with a colour per vertex and a centre colour.
fn fan(m: &mut Mesh, pts: &[Pos2], cols: &[Color32], centre: Color32) {
    let c = pts.iter().fold(Pos2::ZERO, |a, p| a + p.to_vec2()) / pts.len() as f32;
    let i0 = m.vertices.len() as u32;
    m.colored_vertex(c, centre);
    for (p, col) in pts.iter().zip(cols) {
        m.colored_vertex(*p, *col);
    }
    let n = pts.len() as u32;
    for i in 0..n {
        m.add_triangle(i0, i0 + 1 + i, i0 + 1 + (i + 1) % n);
    }
}

/// Soft edge: a strip from `inner` (colour `c`) to `outer` (transparent).
fn feather(m: &mut Mesh, inner: &[Pos2], outer: &[Pos2], c: Color32) {
    let n = inner.len();
    for i in 0..n {
        let j = (i + 1) % n;
        quad(m, [inner[i], inner[j], outer[j], outer[i]], [c, c, Color32::TRANSPARENT, Color32::TRANSPARENT]);
    }
}

fn grow(pts: &[[f32; 2]], d: f32) -> Vec<[f32; 2]> {
    let ns = normals(pts);
    (0..pts.len()).map(|i| {
        let a = ns[(i + pts.len() - 1) % pts.len()];
        let b = ns[i];
        let (nx, ny) = (a[0] + b[0], a[1] + b[1]);
        let l = (nx * nx + ny * ny).sqrt().max(1e-6);
        [pts[i][0] + nx / l * d, pts[i][1] + ny / l * d]
    }).collect()
}

fn inside(poly: &[Pos2], q: Pos2) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.y > q.y) != (b.y > q.y) && q.x < (b.x - a.x) * (q.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
    }
    c
}

// ---------------------------------------------------------------- legends

struct Legend {
    text: &'static str,
    icon: bool,
    size: f32,
    at: [f32; 2], // position on the cap top, 0..1
    centred: bool,
}

const fn l(text: &'static str, size: f32, x: f32, y: f32) -> Legend {
    Legend { text, icon: false, size, at: [x, y], centred: false }
}

const fn ic(text: &'static str, size: f32, x: f32, y: f32, centred: bool) -> Legend {
    Legend { text, icon: true, size, at: [x, y], centred }
}

fn pair(top: &'static str, bottom: &'static str) -> Vec<Legend> {
    vec![l(top, 8.6, 0.15, 0.06), l(bottom, 8.6, 0.15, 0.48)]
}

fn legends(k: &Key) -> Vec<Legend> {
    use egui_phosphor::regular as ph;
    match k.vk {
        0x1B => vec![l("Esc", 7.2, 0.13, 0.12)],
        0x70..=0x7B => vec![l(k.label, 7.0, 0.14, 0.12)],
        0xC0 => pair("~", "`"),
        0x31 => pair("!", "1"),
        0x32 => pair("@", "2"),
        0x33 => pair("#", "3"),
        0x34 => pair("$", "4"),
        0x35 => pair("%", "5"),
        0x36 => pair("^", "6"),
        0x37 => pair("&", "7"),
        0x38 => pair("*", "8"),
        0x39 => pair("(", "9"),
        0x30 => pair(")", "0"),
        0xBD => pair("_", "-"),
        0xBB => pair("+", "="),
        0xDB => pair("{", "["),
        0xDD => pair("}", "]"),
        0xDC => pair("|", "\\"),
        0xBA => pair(":", ";"),
        0xDE => pair("\"", "'"),
        0xBC => pair("<", ","),
        0xBE => pair(">", "."),
        0xBF => pair("?", "/"),
        0x08 => vec![ic(ph::ARROW_LEFT, 6.8, 0.10, 0.30, false), l("Backspace", 6.8, 0.22, 0.30)],
        0x2E => vec![l("Delete", 6.5, 0.14, 0.30)],
        0x21 => vec![l("PgUp", 6.5, 0.14, 0.30)],
        0x22 => vec![l("PgDn", 6.5, 0.14, 0.30)],
        0x23 => vec![l("End", 6.5, 0.14, 0.30)],
        0x09 => vec![ic(ph::ARROW_LINE_LEFT, 6.8, 0.10, 0.04, false), l("Tab", 6.8, 0.10, 0.34), ic(ph::ARROW_LINE_RIGHT, 6.8, 0.10, 0.62, false)],
        0x14 => vec![l("Caps Lock", 6.8, 0.08, 0.30)],
        0x0D => vec![ic(ph::ARROW_ELBOW_DOWN_LEFT, 7.5, 0.10, 0.26, false), l("Enter", 7.0, 0.24, 0.30)],
        0xA0 | 0xA1 => vec![ic(ph::ARROW_FAT_UP, 8.0, 0.08, 0.24, false), l("Shift", 7.0, if k.vk == 0xA0 { 0.22 } else { 0.30 }, 0.30)],
        0xA2 | 0xA3 => vec![l("Ctrl", 6.8, 0.14, 0.30)],
        0x5B => vec![l("Win", 6.8, 0.20, 0.30)],
        0xA4 => vec![l("Alt", 6.8, 0.20, 0.30)],
        0xFA => vec![l("Fn", 6.8, 0.20, 0.30)],
        0x25 => vec![ic(ph::ARROW_LEFT, 7.0, 0.5, 0.45, true)],
        0x26 => vec![ic(ph::ARROW_UP, 7.0, 0.5, 0.45, true)],
        0x27 => vec![ic(ph::ARROW_RIGHT, 7.0, 0.5, 0.45, true)],
        0x28 => vec![ic(ph::ARROW_DOWN, 7.0, 0.5, 0.45, true)],
        0x20 => vec![],
        _ => vec![l(k.label, 10.5, 0.16, 0.06)],
    }
}

/// Prints `text` flat onto the cap top `top` (plane z = `zf(y)`), so it follows the 3D view.
fn print(painter: &Painter, view: &View, top: [f32; 4], zf: &dyn Fn(f32) -> f32, lg: &Legend, colour: Color32, tex: [usize; 2]) {
    let family = if lg.icon { FontFamily::Proportional } else { FontFamily::Name("display".into()) };
    let (w, h) = (top[2] - top[0], top[3] - top[1]);
    let flat = view.cam.pitch.abs() < 0.01 && view.cam.yaw.abs() < 0.01;
    // Legends are always printed on the cap surface, so they move, scale and tilt with the key.
    // In 3D the print is a bit larger (fixed, so it never changes while zooming or turning) to
    // stay readable once tilted; it eases in over the first few degrees of the 2D/3D switch.
    let size = lg.size * (1.0 + 0.25 * (view.cam.pitch.abs() / 0.12).min(1.0));
    let o = view.p([top[0], top[1], zf(top[1])]);
    let screen = size * (view.p([top[0] + 1.0, top[1], zf(top[1])]) - o).length();
    let px = if flat || screen < 16.0 {
        screen.clamp(4.0, 60.0).round()
    } else {
        // zoomed in: rendered larger and in fixed size steps, so it stays sharp without filling
        // the font atlas with every zoom level
        let want = (screen * 1.6).clamp(16.0, 160.0);
        (8.0 * 1.25f32.powf(((want / 8.0).ln() / 1.25f32.ln()).ceil())).round()
    };
    let galley = painter.layout_no_wrap(lg.text.to_string(), FontId::new(px, family), colour);
    let k = size / px; // galley points -> canvas units
    let mut ox = top[0] + lg.at[0] * w;
    let mut oy = top[1] + lg.at[1] * h;
    if lg.centred {
        ox -= galley.size().x * k / 2.0;
        oy -= galley.size().y * k / 2.0;
    }
    let uv = vec2(1.0 / tex[0] as f32, 1.0 / tex[1] as f32);
    let mut m = Mesh::with_texture(TextureId::default());
    for row in &galley.rows {
        let base = m.vertices.len() as u32;
        let rm = &row.visuals.mesh;
        for v in &rm.vertices {
            let x = ox + (row.pos.x + v.pos.x) * k;
            let y = oy + (row.pos.y + v.pos.y) * k;
            let mut nv = *v;
            nv.pos = view.p([x, y, zf(y)]);
            nv.uv = pos2(v.uv.x * uv.x, v.uv.y * uv.y);
            m.vertices.push(nv);
        }
        m.indices.extend(rm.indices.iter().map(|i| i + base));
    }
    painter.add(Shape::mesh(m));
}

// ---------------------------------------------------------------- public API

pub struct Marks {
    pub hovered: Option<usize>,
    pub selected: Option<(usize, f32)>, // key, pulse 0..1
    pub remapped: Vec<bool>,
    pub accent: Color32,
}

struct KeyGeo {
    foot: Vec<[f32; 2]>,
    top: Vec<[f32; 2]>,
    top_rect: [f32; 4],
    z0: f32, // cap bottom at the key centre
}

fn key_geo(k: &Key) -> KeyGeo {
    let f = footprint(k);
    let t = top_face(f);
    let cy = (f[1] + f[3]) / 2.0;
    KeyGeo { foot: rounded(f, 3.5), top: rounded(t, 4.0), top_rect: t, z0: plate_z(cy) + 2.0 }
}

/// Which key is under the pointer (front-most first).
pub fn hit(view: &View, q: Pos2) -> Option<usize> {
    let mut order: Vec<usize> = (0..KEYS.len()).collect();
    order.sort_by(|&a, &b| {
        let (fa, fb) = (footprint(&KEYS[a]), footprint(&KEYS[b]));
        let da = view.depth((fa[0] + fa[2]) / 2.0, (fa[1] + fa[3]) / 2.0);
        let db = view.depth((fb[0] + fb[2]) / 2.0, (fb[1] + fb[3]) / 2.0);
        db.total_cmp(&da)
    });
    for i in order {
        let g = key_geo(&KEYS[i]);
        let top: Vec<Pos2> = g.top.iter().map(|p| view.p([p[0], p[1], g.z0 + CAP_H])).collect();
        let foot: Vec<Pos2> = g.foot.iter().map(|p| view.p([p[0], p[1], g.z0])).collect();
        if inside(&top, q) || inside(&foot, q) {
            return Some(i);
        }
    }
    None
}

/// Centre and radius (screen) of the volume knob's top.
pub fn knob(view: &View) -> (Pos2, f32) {
    let z = case_z(KNOB[1]) + 12.0;
    let c = view.p([KNOB[0], KNOB[1], z]);
    let e = view.p([KNOB[0] + KNOB_R, KNOB[1], z]);
    (c, (e - c).length())
}

pub fn paint(painter: &Painter, view: &View, colours: &[[u8; 3]], marks: &Marks, brightness: f32, knob_hot: bool) {
    let tex = painter.fonts(|f| f.font_image_size());
    let light = [-0.35f32, -0.94f32];
    let shade = |n: [f32; 2]| 0.62 + 0.30 * (n[0] * light[0] + n[1] * light[1]);
    let mut lit_sum = [0u32; 3];
    let mut lit_n = 0u32;
    for c in colours {
        if c.iter().any(|v| *v > 8) {
            lit_n += 1;
            for k in 0..3 {
                lit_sum[k] += c[k] as u32;
            }
        }
    }

    // floor shadow and underglow
    let case = rounded(CASE, 18.0);
    let mut m = Mesh::default();
    let floor: Vec<Pos2> = case.iter().map(|p| view.p([p[0], p[1] + 6.0, 0.0])).collect();
    let floor_out: Vec<Pos2> = grow(&case, 16.0).iter().map(|p| view.p([p[0], p[1] + 10.0, 0.0])).collect();
    fan(&mut m, &floor, &vec![Color32::from_black_alpha(120); floor.len()], Color32::from_black_alpha(120));
    feather(&mut m, &floor, &floor_out, Color32::from_black_alpha(120));
    if lit_n > 0 {
        let g = [(lit_sum[0] / lit_n) as u8, (lit_sum[1] / lit_n) as u8, (lit_sum[2] / lit_n) as u8];
        let wide: Vec<Pos2> = grow(&case, 30.0).iter().map(|p| view.p([p[0], p[1], 0.0])).collect();
        feather(&mut m, &floor_out, &wide, glow(g, 0.08));
    }
    painter.add(Shape::mesh(m));

    // case: walls, top surface, tray opening, plate
    let mut m = Mesh::default();
    let cn = normals(&case);
    for i in 0..case.len() {
        let j = (i + 1) % case.len();
        if view.faces(cn[i], 0.0) <= 0.0 {
            continue;
        }
        let (a, b) = (case[i], case[j]);
        let c = mul(CHASSIS, shade(cn[i]) * 0.9);
        quad(&mut m, [view.p([a[0], a[1], 0.0]), view.p([b[0], b[1], 0.0]), view.p([b[0], b[1], case_z(b[1])]), view.p([a[0], a[1], case_z(a[1])])],
            [mul(c, 0.7), mul(c, 0.7), c, c]);
    }
    let top: Vec<Pos2> = case.iter().map(|p| view.p([p[0], p[1], case_z(p[1])])).collect();
    let cols: Vec<Color32> = case.iter().map(|p| mul(CHASSIS, 1.15 - 0.25 * (p[1] - CASE[1]) / (CASE[3] - CASE[1]))).collect();
    fan(&mut m, &top, &cols, CHASSIS);
    let tray = rounded(TRAY, 8.0);
    let tn = normals(&tray);
    for i in 0..tray.len() {
        let j = (i + 1) % tray.len();
        let inward = [-tn[i][0], -tn[i][1]];
        if view.faces(inward, 0.0) <= 0.0 {
            continue;
        }
        let (a, b) = (tray[i], tray[j]);
        let c = mul(CHASSIS, shade(inward) * 0.55);
        quad(&mut m, [view.p([a[0], a[1], case_z(a[1])]), view.p([b[0], b[1], case_z(b[1])]), view.p([b[0], b[1], plate_z(b[1])]), view.p([a[0], a[1], plate_z(a[1])])],
            [c, c, mul(c, 0.6), mul(c, 0.6)]);
    }
    let plate: Vec<Pos2> = tray.iter().map(|p| view.p([p[0], p[1], plate_z(p[1])])).collect();
    fan(&mut m, &plate, &vec![PLATE; plate.len()], PLATE);
    painter.add(Shape::mesh(m));
    painter.add(Shape::closed_line(top.clone(), Stroke::new(1.0, Color32::from_white_alpha(14))));
    let tray_top: Vec<Pos2> = tray.iter().map(|p| view.p([p[0], p[1], case_z(p[1])])).collect();
    painter.add(Shape::closed_line(tray_top, Stroke::new(1.0, Color32::from_white_alpha(10))));
    // indicator bar between Esc and F1
    let bar = rounded([77.0, 31.0, 80.5, 56.0], 1.7);
    let bar_p: Vec<Pos2> = bar.iter().map(|p| view.p([p[0], p[1], plate_z(p[1]) + 4.0])).collect();
    painter.add(Shape::convex_polygon(bar_p, Color32::from_rgb(0x2a, 0x2b, 0x31), Stroke::NONE));

    // LED light on the plate around each cap (the caps themselves are opaque)
    let mut m = Mesh::default();
    for k in KEYS {
        let c = colours[k.led as usize];
        let lit = c[0].max(c[1]).max(c[2]) as f32 / 255.0;
        if lit < 0.02 {
            continue;
        }
        let g = key_geo(k);
        let inner: Vec<Pos2> = g.foot.iter().map(|p| view.p([p[0], p[1], plate_z(p[1])])).collect();
        let outer: Vec<Pos2> = grow(&g.foot, 7.0).iter().map(|p| view.p([p[0], p[1], plate_z(p[1])])).collect();
        fan(&mut m, &inner, &vec![glow(c, 0.8 * lit); inner.len()], glow(c, 0.8 * lit));
        feather(&mut m, &inner, &outer, glow(c, 0.42 * lit));
    }
    painter.add(Shape::mesh(m));

    // keys and the knob, back to front
    let mut items: Vec<(f32, Option<usize>)> = KEYS.iter().enumerate().map(|(i, k)| {
        let f = footprint(k);
        (view.depth((f[0] + f[2]) / 2.0, (f[1] + f[3]) / 2.0), Some(i))
    }).collect();
    items.push((view.depth(KNOB[0], KNOB[1]), None));
    items.sort_by(|a, b| a.0.total_cmp(&b.0));

    for (_, it) in items {
        let Some(i) = it else {
            paint_knob(painter, view, brightness, knob_hot, marks.accent, light);
            continue;
        };
        let k = &KEYS[i];
        let g = key_geo(k);
        let base = cap_colour(k);
        let c = colours[k.led as usize];
        let led = Color32::from_rgb(c[0], c[1], c[2]);
        let lit = c[0].max(c[1]).max(c[2]) as f32 / 255.0;
        let hot = marks.hovered == Some(i);
        let face = if hot { mul(base, 1.25) } else { base };
        let z1 = g.z0 + CAP_H;
        let zf = |_y: f32| z1;

        let mut m = Mesh::default();
        let n = g.foot.len();
        let ns = normals(&g.foot);
        for s in 0..n {
            let t = (s + 1) % n;
            if view.faces(ns[s], 0.28) <= 0.0 {
                continue;
            }
            let side = mul(face, shade(ns[s]) * 0.78);
            let lo = mix(mul(side, 0.8), led, 0.55 * lit);
            let hi = mix(side, led, 0.10 * lit);
            quad(&mut m, [view.p([g.foot[s][0], g.foot[s][1], g.z0]), view.p([g.foot[t][0], g.foot[t][1], g.z0]),
                view.p([g.top[t][0], g.top[t][1], z1]), view.p([g.top[s][0], g.top[s][1], z1])], [lo, lo, hi, hi]);
        }
        let top: Vec<Pos2> = g.top.iter().map(|p| view.p([p[0], p[1], z1])).collect();
        let (ty0, ty1) = (g.top_rect[1], g.top_rect[3]);
        let cols: Vec<Color32> = g.top.iter().map(|p| mul(face, 1.10 - 0.16 * (p[1] - ty0) / (ty1 - ty0))).collect();
        fan(&mut m, &top, &cols, mul(face, 1.0));
        painter.add(Shape::mesh(m));
        painter.add(Shape::closed_line(top.clone(), Stroke::new(0.8, mul(face, 1.45).gamma_multiply(0.6))));

        let ink = if base == YELLOW { LEGEND_DARK } else { LEGEND };
        for lg in legends(k) {
            print(painter, view, g.top_rect, &zf, &lg, ink, tex);
        }
        if marks.remapped.get(i).copied().unwrap_or(false) {
            let p = view.p([g.top_rect[2] - 4.0, g.top_rect[1] + 4.0, z1]);
            painter.circle_filled(p, 3.0, marks.accent);
        }
        if let Some((s, pulse)) = marks.selected {
            if s == i {
                painter.add(Shape::closed_line(top, Stroke::new(2.0, marks.accent.gamma_multiply(pulse))));
            }
        }
    }
}

fn paint_knob(painter: &Painter, view: &View, brightness: f32, hot: bool, accent: Color32, light: [f32; 2]) {
    let z0 = case_z(KNOB[1]);
    let z1 = z0 + 12.0;
    let ring = circle(KNOB, KNOB_R, 40);
    let ns = normals(&ring);
    let mut m = Mesh::default();
    // brightness arc printed on the case around the knob
    let arc_r = KNOB_R + 5.5;
    let (a0, a1) = (std::f32::consts::PI * 0.75, std::f32::consts::PI * 2.25);
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        (0..=36).map(|i| {
            let a = from + (to - from) * i as f32 / 36.0;
            view.p([KNOB[0] + arc_r * a.cos(), KNOB[1] + arc_r * a.sin(), z0])
        }).collect()
    };
    painter.add(Shape::line(arc(a0, a1), Stroke::new(2.5, Color32::from_white_alpha(18))));
    if brightness > 0.0 {
        let a = a0 + (a1 - a0) * brightness;
        painter.add(Shape::line(arc(a0, a), Stroke::new(6.0, accent.gamma_multiply(0.12))));
        painter.add(Shape::line(arc(a0, a), Stroke::new(2.5, accent)));
    }
    for s in 0..ring.len() {
        let t = (s + 1) % ring.len();
        if view.faces(ns[s], 0.0) <= 0.0 {
            continue;
        }
        let d = ns[s][0] * light[0] + ns[s][1] * light[1];
        let spec = d.max(0.0).powi(6) * 0.9;
        let c = mul(SILVER, 0.55 + 0.45 * d.max(-0.3) + spec);
        quad(&mut m, [view.p([ring[s][0], ring[s][1], z0]), view.p([ring[t][0], ring[t][1], z0]),
            view.p([ring[t][0], ring[t][1], z1]), view.p([ring[s][0], ring[s][1], z1])], [mul(c, 0.7), mul(c, 0.7), c, c]);
    }
    // silver rim on top, dark cap in the middle
    let top: Vec<Pos2> = ring.iter().map(|p| view.p([p[0], p[1], z1])).collect();
    let cols: Vec<Color32> = ns.iter().map(|n| {
        let d = n[0] * light[0] + n[1] * light[1];
        mul(SILVER, 0.75 + 0.6 * d.max(0.0))
    }).collect();
    fan(&mut m, &top, &cols, SILVER);
    let cap = circle(KNOB, KNOB_R * 0.74, 40);
    let cap_p: Vec<Pos2> = cap.iter().map(|p| view.p([p[0], p[1], z1 + 0.5])).collect();
    let cap_c: Vec<Color32> = cap.iter().map(|p| mul(KNOB_CAP, 1.0 + 0.5 * ((KNOB[1] - p[1]) / KNOB_R).max(0.0))).collect();
    fan(&mut m, &cap_p, &cap_c, KNOB_CAP);
    painter.add(Shape::mesh(m));
    painter.add(Shape::closed_line(top, Stroke::new(1.0, if hot { accent } else { Color32::from_white_alpha(40) })));
    let a = a0 + (a1 - a0) * brightness;
    let (s, c) = a.sin_cos();
    painter.line_segment([view.p([KNOB[0] + c * 4.0, KNOB[1] + s * 4.0, z1 + 0.6]), view.p([KNOB[0] + c * 11.0, KNOB[1] + s * 11.0, z1 + 0.6])],
        Stroke::new(2.0, accent));
}
