//! App state: the loaded build, the camera, tools, drafts and the threads sent back to Claude.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use glam::{Mat4, Vec2, Vec3};
use rune::{Spring, TextEdit, Tx, Waker};
use serde_json::{Value, json};

use crate::camera::{Anim, CamAnim, Lens, Orbit, bounds, pose};
use crate::data::{self, Part, Scene, SnapKind, Thread};

pub const SIDEBAR_W: f32 = 380.0;
pub const MARGIN: f32 = 12.0;
const MORPH: Spring = Spring::new(0.75, 0.18);
const SLIDE: Spring = Spring::new(0.35, 0.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Navigate,
    Pin,
    Measure,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    Plate,
    Assembly,
}

impl Layout {
    /// The name comments store, as the web app writes it.
    pub fn name(self) -> &'static str {
        match self {
            Layout::Plate => "print",
            Layout::Assembly => "assembly",
        }
    }

    fn t(self) -> f32 {
        if self == Layout::Assembly { 1.0 } else { 0.0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Comments,
    Report,
    Features,
    Parts,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub part: usize,
    pub world: Vec3,
    pub local: Vec3,
    pub normal: Vec3,
    pub snap: Option<(SnapKind, Option<f32>)>,
}

pub struct Shot {
    pub data_url: String,
    pub pixels: rune::Pixels,
    pub size: (u32, u32),
}

pub struct Draft {
    pub key: u64,
    pub anchor: Value,
    pub camera: Value,
    pub edit: TextEdit,
    pub shot: Option<Shot>,
}

#[derive(Default)]
pub struct Reply {
    pub edit: TextEdit,
    pub status: Option<&'static str>,
}

/// What other threads hand back: screenshots and the server's answer to a send.
#[derive(Default)]
pub struct Shared {
    pub shots: Vec<(u64, Shot)>,
    pub notes: Vec<String>,
    pub sent: Option<Result<Value, String>>,
    pub waker: Option<Waker>,
}

impl Shared {
    pub fn wake(&self) {
        if let Some(w) = &self.waker {
            w();
        }
    }
}

/// One per process: the compositor exists before `init` builds the app, and both need it.
pub static SHARED: std::sync::LazyLock<Arc<Mutex<Shared>>> = std::sync::LazyLock::new(Default::default);

/// A screenshot the compositor should take of the 3D view, with markers drawn at `marks`.
#[derive(Clone)]
pub struct ShotRequest {
    pub id: u64,
    pub marks: Vec<Vec3>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gesture {
    Pending { button_middle: bool, shift: bool },
    Rotate,
    Pan,
}

pub struct Locate {
    pub world: Vec3,
    pub label: String,
}

pub struct App {
    pub project: PathBuf,
    pub server: Option<String>,
    pub scene: Option<Arc<Scene>>,
    pub error: Option<String>,
    stamps: (Option<SystemTime>, Option<SystemTime>),
    next_check: f64,
    pub threads: Vec<Thread>,
    pub cam: CamAnim,
    cams: [Option<Orbit>; 2],
    pub layout: Layout,
    pub morph: Anim,
    pub explode: Anim,
    pub asm_center: Vec3,
    pub section: Option<usize>,
    pub section_t: f32,
    pub section_flip: bool,
    pub mode: Mode,
    pub last_tool: Mode,
    pub solo: Option<String>,
    pub hover: Option<Hit>,
    pub pointer: Option<Vec2>,
    pub gesture: Option<Gesture>,
    drag_last: Vec2,
    pub vp: [f32; 4],
    pub tab: Tab,
    pub drafts: Vec<Draft>,
    pub replies: BTreeMap<i64, Reply>,
    pub reply_open: Option<i64>,
    pub show_resolved: bool,
    pub show_labels: bool,
    pub measure: Vec<Hit>,
    pub locate: Option<Locate>,
    pub tour: Option<usize>,
    pub hot_thread: Option<i64>,
    pub hot_feature: Option<usize>,
    pub toast: Option<(String, f64)>,
    pub shared: Arc<Mutex<Shared>>,
    pub shot_req: Option<ShotRequest>,
    pub clock: f64,
    next_key: u64,
    pub sending: bool,
    closing_at: Option<f64>,
    pub help: bool,
    fitted: bool,
    was_moving: bool,
    scratch: TextEdit,
}

pub fn args() -> (Option<PathBuf>, Option<String>) {
    let a = rune::tooling::extra_args();
    let get = |flag: &str| a.iter().position(|x| x == flag).and_then(|i| a.get(i + 1)).cloned();
    let project = get("--project").map(PathBuf::from).or_else(|| std::env::var_os("CAD_REVIEW_PROJECT").map(PathBuf::from));
    (project, get("--server").or_else(|| std::env::var("CAD_REVIEW_SERVER").ok()))
}

impl App {
    pub fn new() -> App {
        let (project, server) = args();
        App::open(project, server)
    }

    pub fn open(project: Option<PathBuf>, server: Option<String>) -> App {
        let mut app = App {
            project: project.unwrap_or_default(),
            server: server.map(|s| s.trim_end_matches('/').to_owned()),
            scene: None,
            error: None,
            stamps: (None, None),
            next_check: 0.0,
            threads: Vec::new(),
            cam: CamAnim::new(Orbit { target: Vec3::new(128.0, 128.0, 0.0), yaw: -1.0, pitch: 0.5, dist: 500.0 }),
            cams: [None, None],
            layout: Layout::Plate,
            morph: Anim::new(0.0),
            explode: Anim::new(0.0),
            asm_center: Vec3::ZERO,
            section: None,
            section_t: 0.5,
            section_flip: false,
            mode: Mode::Navigate,
            last_tool: Mode::Pin,
            solo: None,
            hover: None,
            pointer: None,
            gesture: None,
            drag_last: Vec2::ZERO,
            vp: [0.0, 0.0, 1440.0, 900.0],
            tab: Tab::Comments,
            drafts: Vec::new(),
            replies: BTreeMap::new(),
            reply_open: None,
            show_resolved: false,
            show_labels: true,
            measure: Vec::new(),
            locate: None,
            tour: None,
            hot_thread: None,
            hot_feature: None,
            toast: None,
            shared: Arc::clone(&SHARED),
            shot_req: None,
            clock: 0.0,
            next_key: 1,
            sending: false,
            closing_at: None,
            help: false,
            fitted: false,
            was_moving: false,
            scratch: TextEdit::new(""),
        };
        if app.project.as_os_str().is_empty() {
            app.error = Some("No project. Start it with `uv run cad review <name> --native`.".into());
        } else {
            app.reload(true);
        }
        app
    }

    // ---------- loading ----------

    fn file_stamps(&self) -> (Option<SystemTime>, Option<SystemTime>) {
        (data::mtime(&data::view_path(&self.project)), data::mtime(&data::comments_path(&self.project)))
    }

    /// Reloads what changed on disk; `force` reloads everything.
    pub fn reload(&mut self, force: bool) -> bool {
        let stamps = self.file_stamps();
        let mut changed = false;
        if force || stamps.0 != self.stamps.0 {
            let generation = self.scene.as_ref().map_or(1, |s| s.generation + 1);
            match data::load(&self.project, generation) {
                Ok(scene) => {
                    let had = self.scene.is_some();
                    self.asm_center = scene_center(&scene);
                    if !scene.has_assembly && self.layout == Layout::Assembly {
                        self.layout = Layout::Plate;
                        self.morph.set(0.0);
                    }
                    if self.solo.as_ref().is_some_and(|k| !scene.parts.iter().any(|p| &p.key == k)) {
                        self.solo = None;
                    }
                    self.scene = Some(Arc::new(scene));
                    self.error = None;
                    self.measure.clear();
                    self.hover = None;
                    if had {
                        self.toast("Reloaded the new build");
                    }
                }
                Err(e) => self.error = Some(e),
            }
            changed = true;
        }
        if force || stamps.1 != self.stamps.1 {
            self.threads = data::load_threads(&self.project);
            changed = true;
        }
        self.stamps = stamps;
        changed
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), self.clock + 2.6));
    }

    // ---------- per frame ----------

    /// Advances springs, picks up other threads' results and checks the build on disk.
    pub fn step(&mut self, t: f64, tx: &mut Tx) -> bool {
        let dt = ((t - self.clock).clamp(0.0, 0.1)) as f32;
        self.clock = t;
        let mut changed = false;
        if !self.fitted && self.vp[2] > 1.0 {
            self.fitted = true;
            if let Some(sc) = &self.scene {
                tx.set_title(format!("{} · claud3d review", sc.project));
            }
            self.fit(false);
            changed = true;
        }
        changed |= self.cam.step(dt);
        changed |= self.morph.step(MORPH, dt);
        changed |= self.explode.step(SLIDE, dt);
        // Labels hide behind the model only at rest, so the frame motion stops needs a view too.
        let moving = self.animating();
        changed |= self.was_moving && !moving;
        self.was_moving = moving;
        if t >= self.next_check {
            self.next_check = t + 1.0;
            if self.file_stamps() != self.stamps {
                changed |= self.reload(false);
            }
            tx.wake_after(std::time::Duration::from_secs(1));
        }
        let (shots, sent, notes) = {
            let mut sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (std::mem::take(&mut sh.shots), sh.sent.take(), std::mem::take(&mut sh.notes))
        };
        for n in notes {
            self.toast(n);
            changed = true;
        }
        for (key, shot) in shots {
            if let Some(d) = self.drafts.iter_mut().find(|d| d.key == key) {
                d.shot = Some(shot);
                changed = true;
            }
        }
        if let Some(r) = sent {
            self.sending = false;
            changed = true;
            match r {
                Ok(v) => self.sent(v),
                Err(e) => self.toast(format!("Send failed: {e}")),
            }
        }
        if let Some((_, until)) = &self.toast
            && t > *until
        {
            self.toast = None;
            changed = true;
        }
        if let Some(at) = self.closing_at
            && t >= at
        {
            tx.quit();
        }
        changed
    }

    pub fn animating(&self) -> bool {
        self.cam.goal.is_some() || !self.morph.settled() || !self.explode.settled()
    }

    // ---------- geometry ----------

    pub fn lens(&self) -> Lens {
        Lens { w: self.vp[2], h: self.vp[3], shift: (SIDEBAR_W + MARGIN) * 0.5 }
    }

    /// Each drawn part: index, part, world matrix and opacity.
    pub fn posed(&self) -> Vec<(usize, &Part, Mat4, f32)> {
        let Some(scene) = &self.scene else { return Vec::new() };
        scene
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (m, mut a) = pose(p, self.morph.x, self.explode.x, self.asm_center);
                if self.solo.as_ref().is_some_and(|k| k != &p.key) {
                    a = 0.0;
                }
                (i, p, m, a)
            })
            .collect()
    }

    pub fn part(&self, i: usize) -> Option<&Part> {
        self.scene.as_ref()?.parts.get(i)
    }

    pub fn part_matrix(&self, i: usize) -> Mat4 {
        self.part(i).map_or(Mat4::IDENTITY, |p| pose(p, self.morph.x, self.explode.x, self.asm_center).0)
    }

    /// Section plane as (normal, offset): points with `n·p > d` are cut away.
    pub fn plane(&self) -> Option<(Vec3, f32)> {
        let axis = self.section?;
        let (lo, hi) = self.section_range()?;
        let mut n = Vec3::ZERO;
        n[axis] = 1.0;
        let d = lo + (hi - lo) * self.section_t;
        Some(if self.section_flip { (-n, -d) } else { (n, d) })
    }

    pub fn section_range(&self) -> Option<(f32, f32)> {
        let axis = self.section?;
        let posed = self.posed();
        let parts: Vec<&Part> = posed.iter().filter(|x| x.3 > 0.45).map(|x| x.1).collect();
        let (lo, hi) = bounds(&parts, self.morph.x, self.explode.x, self.asm_center)?;
        Some((lo[axis] - 0.01, hi[axis] + 0.01))
    }

    pub fn kept(&self, w: Vec3) -> bool {
        self.plane().is_none_or(|(n, d)| n.dot(w) <= d)
    }

    /// The model under viewport point `p`: printed parts first, then the ghosted context parts.
    pub fn pick(&self, p: Vec2) -> Option<Hit> {
        self.pick_in(p, true).or_else(|| self.pick_in(p, false))
    }

    fn pick_in(&self, p: Vec2, printed: bool) -> Option<Hit> {
        let lens = self.lens();
        let (o, d) = self.cam.now.ray(p.x, p.y, &lens);
        let plane = self.plane();
        let mut best: Option<(f32, Hit)> = None;
        for (i, part, m, a) in self.posed() {
            if a < 0.45 || part.printed != printed {
                continue;
            }
            let inv = m.inverse();
            let (lo, ld) = (inv.transform_point3(o), inv.transform_vector3(d));
            let keep = |l: Vec3| plane.is_none_or(|(n, dd)| n.dot(m.transform_point3(l)) <= dd);
            if let Some((t, tri)) = part.mesh.raycast(lo, ld, best.as_ref().map_or(f32::MAX, |b| b.0), &keep) {
                let local = lo + ld * t;
                let mut n = part.mesh.normals[tri];
                if n.dot(ld) > 0.0 {
                    n = -n;
                }
                best = Some((t, Hit { part: i, world: m.transform_point3(local), local, normal: n, snap: None }));
            }
        }
        best.map(|b| b.1)
    }

    /// Like `pick`, but snapped to the nearest B-rep vertex, circle centre or edge midpoint
    /// within 12 points of the pointer.
    pub fn pick_snapped(&self, p: Vec2, free: bool) -> Option<Hit> {
        let face = self.pick(p);
        if free {
            return face;
        }
        let lens = self.lens();
        let mut best: Option<(f32, Hit)> = None;
        for (i, part, m, a) in self.posed() {
            if a < 0.45 {
                continue;
            }
            for s in &part.snaps {
                let w = m.transform_point3(s.p);
                let Some(sp) = self.cam.now.project(w, &lens) else { continue };
                let rank = match s.kind {
                    SnapKind::Vertex | SnapKind::Center => 0.0,
                    SnapKind::Midpoint => 3.0,
                };
                let dist = sp.distance(p) + rank;
                if dist < 12.0 && best.as_ref().is_none_or(|b| dist < b.0) && self.kept(w) && (s.kind == SnapKind::Center || !self.occluded(w)) {
                    let n = face.as_ref().filter(|f| f.part == i).map_or(Vec3::Z, |f| f.normal);
                    best = Some((dist, Hit { part: i, world: w, local: s.p, normal: n, snap: Some((s.kind, s.r)) }));
                }
            }
        }
        best.map(|b| b.1).or(face)
    }

    /// True when the model hides `w` from the camera.
    pub fn occluded(&self, w: Vec3) -> bool {
        let eye = self.cam.now.eye();
        let to = w - eye;
        let dist = to.length();
        let d = to / dist.max(1e-6);
        let limit = dist - (dist * 0.004).max(0.3);
        let plane = self.plane();
        self.posed().into_iter().any(|(_, part, m, a)| {
            if a < 0.5 || !part.printed {
                return false;
            }
            let inv = m.inverse();
            let keep = |l: Vec3| plane.is_none_or(|(n, dd)| n.dot(m.transform_point3(l)) <= dd);
            part.mesh.raycast(inv.transform_point3(eye), inv.transform_vector3(d), limit, &keep).is_some()
        })
    }

    pub fn project(&self, w: Vec3) -> Option<Vec2> {
        self.cam.now.project(w, &self.lens())
    }

    // ---------- camera ----------

    fn visible_bounds(&self, t: f32) -> Option<(Vec3, Vec3)> {
        let scene = self.scene.as_ref()?;
        let parts: Vec<&Part> = scene.parts.iter().filter(|p| self.solo.as_ref().is_none_or(|k| k == &p.key)).collect();
        bounds(&parts, t, if t > 0.5 { self.explode.to } else { 0.0 }, self.asm_center)
    }

    fn sphere(&self, t: f32) -> (Vec3, f32) {
        self.visible_bounds(t).map_or((Vec3::new(128.0, 128.0, 0.0), 120.0), |(lo, hi)| ((lo + hi) * 0.5, (hi - lo).length() * 0.5))
    }

    pub fn fit(&mut self, animate: bool) {
        let (c, r) = self.sphere(self.layout.t());
        let to = self.cam.now.fit(c, r, self.lens().aspect());
        if animate { self.cam.fly(to) } else { *self.cam.grab() = to }
    }

    pub fn view_preset(&mut self, name: &str) {
        let (c, r) = self.sphere(self.layout.t());
        self.cam.fly(Orbit::preset(name, c, r, self.lens().aspect()));
    }

    pub fn set_layout(&mut self, l: Layout) {
        if l == self.layout || (l == Layout::Assembly && !self.scene.as_ref().is_some_and(|s| s.has_assembly)) {
            return;
        }
        self.cams[self.layout as usize] = Some(self.cam.goal.unwrap_or(self.cam.now));
        self.layout = l;
        self.morph.to = l.t();
        self.measure.clear();
        self.locate = None;
        match self.cams[l as usize] {
            Some(o) => self.cam.fly(o),
            None => {
                let (c, r) = self.sphere(l.t());
                let o = self.cam.now.fit(c, r, self.lens().aspect());
                self.cam.fly(o);
            }
        }
    }

    pub fn set_explode(&mut self, e: f32) {
        self.explode.to = e.clamp(0.0, 1.5);
    }

    pub fn camera_json(&self) -> Value {
        let o = self.cam.goal.unwrap_or(self.cam.now);
        let r = |v: Vec3| json!([round3(v.x), round3(v.y), round3(v.z)]);
        json!({"position": r(o.eye()), "target": r(o.target), "layout": self.layout.name()})
    }

    /// Flies to a camera a comment saved (by this app or the web app).
    pub fn go_to_camera(&mut self, cam: &Value) {
        let (Some(pos), Some(target)) = (data::anchor_vec(cam, "position"), data::anchor_vec(cam, "target")) else { return };
        if let Some(l) = cam["layout"].as_str() {
            self.set_layout(if l == "assembly" { Layout::Assembly } else { Layout::Plate });
        }
        let d = pos - target;
        let dist = d.length().max(1.0);
        self.cam.fly(Orbit { target, yaw: d.y.atan2(d.x), pitch: (d.z / dist).clamp(-1.0, 1.0).asin(), dist });
    }

    /// Flies to look at `w` from along `normal` (or the current direction), at a close range.
    pub fn look_at(&mut self, w: Vec3, normal: Option<Vec3>, radius: f32) {
        let mut o = self.cam.goal.unwrap_or(self.cam.now);
        if let Some(n) = normal.filter(|n| n.length() > 0.5) {
            let n = (n.normalize() + Vec3::new(0.25, -0.35, 0.3)).normalize();
            o.yaw = n.y.atan2(n.x);
            o.pitch = n.z.clamp(-0.98, 0.98).asin();
        }
        o.target = w;
        o.dist = o.fit(w, radius, self.lens().aspect()).dist;
        self.cam.fly(o);
    }

    // ---------- anchors ----------

    pub fn part_index(&self, name: &str, frame: &str) -> Option<usize> {
        let scene = self.scene.as_ref()?;
        let key = if frame == "assembly" { format!("{name}@assembly") } else { name.to_owned() };
        scene.parts.iter().position(|p| p.key == key).or_else(|| scene.parts.iter().position(|p| p.name == name))
    }

    /// Where an anchor's point (or `point_b`) is drawn now.
    pub fn anchor_world(&self, a: &Value, second: bool) -> Option<Vec3> {
        let (pk, ptk) = if second { ("part_b", "point_b") } else { ("part", "point") };
        let p = data::anchor_vec(a, ptk)?;
        let i = self.part_index(a[pk].as_str()?, a["frame"].as_str().unwrap_or("part"))?;
        Some(self.part_matrix(i).transform_point3(p))
    }

    pub fn nearest_feature(&self, part: &str, p: Vec3) -> Option<String> {
        let scene = self.scene.as_ref()?;
        let reach = scene.parts.iter().find(|x| x.name == part).map_or(6.0, |x| (Vec3::from(x.bbox).length() * 0.12).max(6.0));
        scene
            .features
            .iter()
            .filter(|f| f.part == part)
            .map(|f| (f.point.distance(p), &f.name))
            .filter(|(d, _)| *d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, n)| n.clone())
    }

    fn frame_of(&self, i: usize) -> &'static str {
        if self.part(i).is_some_and(|p| !p.printed) { "assembly" } else { "part" }
    }

    fn point_anchor(&self, h: &Hit) -> Value {
        let p = self.part(h.part).map_or("", |p| p.name.as_str()).to_owned();
        let mut a = json!({
            "kind": "point", "part": p, "point": v3(h.local), "normal": v3(h.normal),
            "frame": self.frame_of(h.part), "view": self.layout.name(),
        });
        if let Some(f) = self.nearest_feature(&p, h.local) {
            a["feature"] = json!(f);
        }
        a
    }

    pub fn measure_anchor(&self) -> Option<Value> {
        let [a, b] = self.measure.as_slice() else { return None };
        let name = |h: &Hit| self.part(h.part).map_or(String::new(), |p| p.name.clone());
        let d = b.world - a.world;
        let mut v = json!({
            "kind": "measure", "part": name(a), "point": v3(a.local), "part_b": name(b), "point_b": v3(b.local),
            "frame": self.frame_of(a.part), "distance": round3(d.length()), "delta": v3(d), "view": self.layout.name(),
        });
        if let Some((k, _)) = a.snap {
            v["snap_a"] = json!(k.name());
        }
        if let Some((k, _)) = b.snap {
            v["snap_b"] = json!(k.name());
        }
        if let Some(axis) = axis_of(d) {
            v["axis"] = json!(axis);
        }
        if let Some(f) = self.nearest_feature(&name(a), a.local) {
            v["feature"] = json!(f);
        }
        Some(v)
    }

    // ---------- tools ----------

    pub fn set_mode(&mut self, m: Mode) {
        if m != Mode::Navigate {
            self.last_tool = m;
        }
        self.mode = m;
    }

    pub fn click(&mut self, p: Vec2, free: bool) {
        match self.mode {
            Mode::Navigate => {}
            Mode::Pin => {
                if let Some(h) = self.pick(p) {
                    let a = self.point_anchor(&h);
                    self.create_draft(a, vec![h.world]);
                }
            }
            Mode::Measure => {
                if let Some(h) = self.pick_snapped(p, free) {
                    if self.measure.len() >= 2 {
                        self.measure.clear();
                    }
                    self.measure.push(h);
                }
            }
        }
    }

    pub fn escape(&mut self) -> bool {
        if self.help {
            self.help = false;
        } else if self.tour.is_some() {
            self.tour = None;
        } else if !self.measure.is_empty() {
            self.measure.clear();
        } else if self.locate.is_some() {
            self.locate = None;
        } else if self.mode != Mode::Navigate {
            self.mode = Mode::Navigate;
        } else {
            return false;
        }
        true
    }

    pub fn drag(&mut self, phase: rune::Phase, p: Vec2, travel: Vec2, middle: bool, shift: bool) {
        match phase {
            rune::Phase::Down => {
                self.gesture = Some(Gesture::Pending { button_middle: middle, shift });
                self.drag_last = Vec2::ZERO;
            }
            rune::Phase::Move => {
                let g = match self.gesture {
                    Some(Gesture::Pending { button_middle, shift }) if travel.length() > 3.0 => {
                        let g = if button_middle || shift { Gesture::Pan } else { Gesture::Rotate };
                        self.gesture = Some(g);
                        g
                    }
                    Some(g @ (Gesture::Rotate | Gesture::Pan)) => g,
                    _ => return,
                };
                let d = travel - self.drag_last;
                self.drag_last = travel;
                let h = self.vp[3];
                let o = self.cam.grab();
                match g {
                    Gesture::Rotate => o.rotate(d.x, d.y),
                    _ => o.pan(d.x, d.y, h),
                }
            }
            rune::Phase::Up => {
                if let Some(Gesture::Pending { button_middle: false, shift }) = self.gesture {
                    self.click(p, shift);
                }
                self.gesture = None;
            }
        }
    }

    pub fn wheel(&mut self, p: Vec2, dy: f32, precise: bool) {
        let _ = precise;
        let notches = dy / 40.0;
        let factor = 1.1f32.powf(-notches);
        let anchor = self.pick(p).map(|h| h.world);
        self.cam.grab().zoom_about(factor, anchor);
    }

    // ---------- drafts and sending ----------

    pub fn create_draft(&mut self, anchor: Value, marks: Vec<Vec3>) {
        let key = self.next_key;
        self.next_key += 1;
        let mut edit = TextEdit::new("");
        edit.focused = true;
        self.drafts.push(Draft { key, anchor, camera: self.camera_json(), edit, shot: None });
        self.shot_req = Some(ShotRequest { id: key, marks });
        self.tab = Tab::Comments;
    }

    pub fn draft_edit(&mut self, key: u64) -> &mut TextEdit {
        match self.drafts.iter().position(|d| d.key == key) {
            Some(i) => &mut self.drafts[i].edit,
            None => &mut self.scratch,
        }
    }

    pub fn reply_edit(&mut self, id: i64) -> &mut TextEdit {
        &mut self.replies.entry(id).or_default().edit
    }

    pub fn general_draft(&mut self) {
        self.create_draft(Value::Null, Vec::new());
    }

    pub fn measure_draft(&mut self) {
        if let Some(a) = self.measure_anchor() {
            let marks = self.measure.iter().map(|h| h.world).collect();
            self.create_draft(a, marks);
        }
    }

    pub fn pending(&self) -> (usize, usize) {
        let n = self.drafts.iter().filter(|d| !d.edit.text.trim().is_empty()).count();
        let m = self.replies.values().filter(|r| !r.edit.text.trim().is_empty() || r.status.is_some()).count();
        (n, m)
    }

    pub fn payload(&self) -> Value {
        let new: Vec<Value> = self
            .drafts
            .iter()
            .filter(|d| !d.edit.text.trim().is_empty())
            .map(|d| {
                json!({
                    "anchor": d.anchor, "camera": d.camera, "text": d.edit.text.trim(),
                    "screenshot": d.shot.as_ref().map(|s| s.data_url.clone()),
                })
            })
            .collect();
        let replies: Vec<Value> = self
            .replies
            .iter()
            .filter(|(_, r)| !r.edit.text.trim().is_empty() || r.status.is_some())
            .map(|(id, r)| json!({"id": id, "text": r.edit.text.trim(), "status": r.status}))
            .collect();
        json!({"new": new, "replies": replies})
    }

    pub fn send(&mut self) {
        let Some(server) = self.server.clone() else {
            self.toast("No review server: start this with `uv run cad review <name> --native`");
            return;
        };
        let (n, m) = self.pending();
        if n + m == 0 || self.sending || self.drafts.iter().any(|d| d.shot.is_none() && !d.edit.text.trim().is_empty()) {
            return;
        }
        self.sending = true;
        let body = self.payload().to_string();
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let r = crate::net::post_json(&server, "/api/send", &body);
            let mut sh = shared.lock().unwrap_or_else(|e| e.into_inner());
            sh.sent = Some(r);
            sh.wake();
        });
    }

    fn sent(&mut self, v: Value) {
        self.drafts.retain(|d| d.edit.text.trim().is_empty());
        self.replies.clear();
        self.reply_open = None;
        self.threads = data::load_threads(&self.project);
        self.stamps = self.file_stamps();
        if v["closing"].as_bool().unwrap_or(false) {
            self.toast("Sent to Claude. Closing…");
            self.closing_at = Some(self.clock + 1.4);
        } else {
            self.toast("Sent to Claude");
        }
    }

    pub fn open_studio(&mut self) {
        let Some(server) = self.server.clone() else {
            self.toast("Run `uv run cad open <name>` to open the sliced project");
            return;
        };
        let shared = self.shared.clone();
        self.toast("Opening Bambu Studio…");
        std::thread::spawn(move || {
            if let Err(e) = crate::net::post_json(&server, "/api/open-studio", "{}") {
                let mut sh = shared.lock().unwrap_or_else(|e| e.into_inner());
                sh.notes.push(e);
                sh.wake();
            }
        });
    }

    pub fn thread_world(&self, t: &Thread) -> Option<Vec3> {
        if t.anchor.is_null() { None } else { self.anchor_world(&t.anchor, false) }
    }

    pub fn draft_world(&self, d: &Draft) -> Option<Vec3> {
        if d.anchor.is_null() { None } else { self.anchor_world(&d.anchor, false) }
    }

    pub fn show_feature(&mut self, i: usize) {
        let Some(scene) = self.scene.clone() else { return };
        let Some(f) = scene.features.get(i) else { return };
        let want = if f.view.as_deref() == Some("assembly") { Layout::Assembly } else { Layout::Plate };
        if want != self.layout && scene.has_assembly {
            self.set_layout(want);
        }
        let Some(pi) = self.part_index(&f.part, "part") else { return };
        let m = pose(&scene.parts[pi], self.layout.t(), if self.layout == Layout::Assembly { self.explode.to } else { 0.0 }, self.asm_center).0;
        let w = m.transform_point3(f.point);
        let n = f.normal.map(|n| m.transform_vector3(n));
        let r = (Vec3::from(scene.parts[pi].bbox).length() * 0.35).clamp(15.0, 120.0);
        self.look_at(w, n, r);
        self.hot_feature = Some(i);
    }

    pub fn tour_step(&mut self, d: i32) {
        let n = self.scene.as_ref().map_or(0, |s| s.features.len()) as i32;
        if n == 0 {
            return;
        }
        let i = self.tour.map_or(0, |i| (i as i32 + d).rem_euclid(n)) as usize;
        self.tour = Some(i);
        self.show_feature(i);
    }

    /// Shows a report location: `at` is in the part's print frame, or the assembly frame for
    /// assembly findings (`part` None).
    pub fn locate(&mut self, part: Option<&str>, at: Vec3, label: String) {
        let w = match part {
            Some(name) => {
                if self.layout != Layout::Plate {
                    self.set_layout(Layout::Plate);
                }
                let Some(i) = self.part_index(name, "part") else { return };
                self.part(i).map_or(at, |p| p.plate.transform_point3(at))
            }
            None => {
                self.set_layout(Layout::Assembly);
                self.set_explode(0.0);
                at
            }
        };
        self.locate = Some(Locate { world: w, label });
        let r = (self.sphere(self.layout.t()).1 * 0.5).clamp(20.0, 90.0);
        self.look_at(w, None, r);
    }

    /// Where the locate marker is now: plate coordinates move with the part while the layout
    /// animates, so they are recomputed from the part the label names.
    pub fn locate_world(&self) -> Option<Vec3> {
        self.locate.as_ref().map(|l| l.world)
    }

    pub fn toggle_solo(&mut self, key: &str) {
        self.solo = if self.solo.as_deref() == Some(key) { None } else { Some(key.to_owned()) };
        self.fit(true);
    }
}

fn scene_center(scene: &Scene) -> Vec3 {
    let parts: Vec<&Part> = scene.parts.iter().collect();
    bounds(&parts, 1.0, 0.0, Vec3::ZERO).map_or(Vec3::ZERO, |(lo, hi)| (lo + hi) * 0.5)
}

pub fn round3(x: f32) -> f64 {
    ((x as f64) * 1000.0).round() / 1000.0
}

pub fn v3(v: Vec3) -> Value {
    json!([round3(v.x), round3(v.y), round3(v.z)])
}

pub fn axis_of(d: Vec3) -> Option<&'static str> {
    let l = d.length();
    if l < 1e-6 {
        return None;
    }
    let a = d.abs() / l;
    [("X", a.x), ("Y", a.y), ("Z", a.z)].into_iter().find(|(_, c)| *c > 0.9999).map(|(n, _)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project with one 10 mm cube, on the plate at (100, 100) and assembled 20 mm up.
    fn project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cad-review-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("out")).unwrap();
        let p = |x: f32, y: f32, z: f32| [x * 10.0 - 5.0, y * 10.0 - 5.0, z * 10.0];
        let quads = [
            [p(0., 0., 0.), p(0., 1., 0.), p(1., 1., 0.), p(1., 0., 0.)],
            [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)],
            [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)],
            [p(0., 1., 0.), p(0., 1., 1.), p(1., 1., 1.), p(1., 1., 0.)],
            [p(0., 0., 0.), p(0., 0., 1.), p(0., 1., 1.), p(0., 1., 0.)],
            [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)],
        ];
        let mut stl = vec![0u8; 80];
        stl.extend(12u32.to_le_bytes());
        for q in quads {
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                stl.extend([0u8; 12]);
                for v in t.iter().flatten() {
                    stl.extend(v.to_le_bytes());
                }
                stl.extend([0u8; 2]);
            }
        }
        std::fs::write(dir.join("out/cube.stl"), stl).unwrap();
        let view = json!({
            "project": "cube", "built_at": "2026-11-08 12:00:00", "commit": "abc", "dirty": false, "status": "PASS",
            "parts": {"cube": {"stl": "cube.stl", "plate_offset": [100, 100, 0], "bbox": [10, 10, 10]}},
            "assembly": {"cube": {"part": "cube", "matrix": [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 20], [0, 0, 0, 1]]}},
            "plate": {"nozzles": [{"name": "left", "x": [0, 256], "y": [0, 256], "height": 250}]},
            "report": {"parts": {"cube": {"status": "PASS", "findings": []}}, "assembly": [], "spec": [], "slice": null},
            "features": [{"name": "top", "part": "cube", "point": [0, 0, 10], "text": "the top face", "params": {}}],
            "params": {"size": 10},
        });
        std::fs::write(dir.join("out/view.json"), view.to_string()).unwrap();
        dir
    }

    #[test]
    fn a_pin_anchors_in_the_print_frame_and_follows_the_part_into_the_assembly() {
        let mut s = App::open(Some(project()), None);
        assert!(s.error.is_none(), "{:?}", s.error);
        s.vp = [0.0, 0.0, 1200.0, 800.0];
        s.fit(false);
        let top = Vec3::new(100.0, 100.0, 10.0);
        let at = s.project(top).unwrap();
        s.set_mode(Mode::Pin);
        s.click(at, false);
        let a = s.drafts[0].anchor.clone();
        assert_eq!((a["part"].as_str(), a["view"].as_str(), a["feature"].as_str()), (Some("cube"), Some("print"), Some("top")));
        let local = data::anchor_vec(&a, "point").unwrap();
        assert!(local.distance(Vec3::new(0.0, 0.0, 10.0)) < 0.01, "{local}");
        assert_eq!(s.shot_req.as_ref().map(|r| r.id), Some(s.drafts[0].key), "a screenshot is on its way");
        s.set_layout(Layout::Assembly);
        s.morph.set(1.0);
        assert!(s.anchor_world(&a, false).unwrap().distance(Vec3::new(0.0, 0.0, 30.0)) < 0.01);
    }

    #[test]
    fn the_payload_carries_drafts_with_text_and_pending_replies() {
        let mut s = App::open(Some(project()), None);
        s.general_draft();
        s.general_draft();
        s.drafts[0].edit.replace_text("  looks good  ");
        s.replies.entry(7).or_default().status = Some("resolved");
        let p = s.payload();
        assert_eq!(p["new"].as_array().map(Vec::len), Some(1));
        assert_eq!(p["new"][0]["text"], "looks good");
        assert_eq!(p["replies"][0], json!({"id": 7, "text": "", "status": "resolved"}));
        assert_eq!(s.pending(), (1, 1));
    }

    #[test]
    fn axis_aligned_measurements_name_their_axis() {
        assert_eq!(axis_of(Vec3::new(0.0, 0.0, -12.5)), Some("Z"));
        assert_eq!(axis_of(Vec3::new(3.0, 4.0, 0.0)), None);
    }
}
