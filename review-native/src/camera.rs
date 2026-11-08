//! Z-up orbit camera, springs that fly it between views, and the plate/assembly pose of each part.

use glam::{Mat4, Quat, Vec2, Vec3};
use rune::Spring;

use crate::data::Part;

pub const FOV: f32 = 35.0 * std::f32::consts::PI / 180.0;
pub const ROTATE_PER_PT: f32 = 0.8 * std::f32::consts::PI / 180.0;
const MAX_PITCH: f32 = 89.5 * std::f32::consts::PI / 180.0;

/// The viewport the camera draws into, in points, with its centre shifted left by `shift`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lens {
    pub w: f32,
    pub h: f32,
    pub shift: f32,
}

impl Lens {
    /// Width over height of the part of the viewport the model centres in.
    pub fn aspect(&self) -> f32 {
        (self.w - 2.0 * self.shift).max(1.0) / self.h.max(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

impl Orbit {
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cp * cy, cp * sy, sp) * self.dist
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Z)
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        let near = (self.dist * 0.01).max(0.05);
        Mat4::perspective_rh(FOV, aspect.max(0.01), near, self.dist * 40.0 + 4000.0)
    }

    /// The camera through `l`: its centre sits `l.shift` points left of the viewport's centre,
    /// so the model centres in the space a floating sidebar leaves.
    pub fn view_proj(&self, l: &Lens) -> Mat4 {
        Mat4::from_translation(Vec3::new(-2.0 * l.shift / l.w.max(1.0), 0.0, 0.0)) * self.proj(l.w / l.h.max(1.0)) * self.view()
    }

    /// Screen right and up in world space.
    pub fn basis(&self) -> (Vec3, Vec3) {
        let fwd = (self.target - self.eye()).normalize();
        let right = fwd.cross(Vec3::Z).normalize_or(Vec3::X);
        (right, right.cross(fwd))
    }

    /// World units per point at the target's depth, for a viewport `h` points tall.
    pub fn units_per_pt(&self, h: f32) -> f32 {
        2.0 * self.dist * (FOV * 0.5).tan() / h.max(1.0)
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * ROTATE_PER_PT;
        self.pitch = (self.pitch + dy * ROTATE_PER_PT).clamp(-MAX_PITCH, MAX_PITCH);
    }

    pub fn pan(&mut self, dx: f32, dy: f32, h: f32) {
        let (right, up) = self.basis();
        let k = self.units_per_pt(h);
        self.target += (-right * dx + up * dy) * k;
    }

    /// Zooms by `factor` (below 1 moves closer), keeping `anchor` where it is on screen.
    pub fn zoom_about(&mut self, factor: f32, anchor: Option<Vec3>) {
        let d = (self.dist * factor).clamp(1.0, 20000.0);
        let k = d / self.dist;
        if let Some(a) = anchor {
            self.target = a + (self.target - a) * k;
        }
        self.dist = d;
    }

    /// A named view looking at `center` from far enough to show a sphere of `radius`.
    pub fn preset(name: &str, center: Vec3, radius: f32, aspect: f32) -> Orbit {
        let deg = std::f32::consts::PI / 180.0;
        let (yaw, pitch) = match name {
            "front" => (-90.0 * deg, 0.0),
            "right" => (0.0, 0.0),
            "top" => (-90.0 * deg, MAX_PITCH),
            "back" => (90.0 * deg, 0.0),
            "left" => (180.0 * deg, 0.0),
            _ => (-55.0 * deg, 30.0 * deg),
        };
        Orbit { target: center, yaw, pitch, dist: fit_dist(radius, aspect) }
    }

    pub fn fit(&self, center: Vec3, radius: f32, aspect: f32) -> Orbit {
        Orbit { target: center, dist: fit_dist(radius, aspect), ..*self }
    }

    /// Where `p` lands in a `w` x `h` viewport (points from its top-left), and whether it is in
    /// front of the camera.
    pub fn project(&self, p: Vec3, l: &Lens) -> Option<Vec2> {
        let c = self.view_proj(l) * p.extend(1.0);
        (c.w > 1e-4).then(|| Vec2::new((c.x / c.w * 0.5 + 0.5) * l.w, (0.5 - c.y / c.w * 0.5) * l.h))
    }

    /// The world ray under viewport point (x, y): origin and unit direction.
    pub fn ray(&self, x: f32, y: f32, l: &Lens) -> (Vec3, Vec3) {
        let inv = self.view_proj(l).inverse();
        let nx = x / l.w.max(1.0) * 2.0 - 1.0;
        let ny = 1.0 - y / l.h.max(1.0) * 2.0;
        let a = inv.project_point3(Vec3::new(nx, ny, 0.0));
        let b = inv.project_point3(Vec3::new(nx, ny, 1.0));
        (a, (b - a).normalize())
    }
}

fn fit_dist(radius: f32, aspect: f32) -> f32 {
    let half = (FOV * 0.5).tan();
    let fov_min = if aspect < 1.0 { (half * aspect).atan() } else { FOV * 0.5 };
    (radius.max(5.0) / fov_min.sin()) * 1.12
}

/// One value moving toward a target on a spring.
#[derive(Clone, Copy, Debug)]
pub struct Anim {
    pub x: f32,
    pub v: f32,
    pub to: f32,
}

impl Anim {
    pub fn new(x: f32) -> Anim {
        Anim { x, v: 0.0, to: x }
    }

    pub fn set(&mut self, x: f32) {
        *self = Anim::new(x);
    }

    pub fn step(&mut self, spring: Spring, dt: f32) -> bool {
        if self.settled() {
            self.x = self.to;
            self.v = 0.0;
            return false;
        }
        (self.x, self.v) = spring.sample(self.x, self.v, self.to, dt);
        true
    }

    pub fn settled(&self) -> bool {
        (self.x - self.to).abs() < 1e-4 && self.v.abs() < 1e-3
    }
}

/// Opacity of assembly-only context parts (boards, counters), so the printed parts show through.
pub const GHOST: f32 = 0.5;

pub const FLY: Spring = Spring::new(0.55, 0.0);

/// The camera, flying on springs toward a goal when one is set.
pub struct CamAnim {
    pub now: Orbit,
    vel: [f32; 6],
    pub goal: Option<Orbit>,
}

impl CamAnim {
    pub fn new(o: Orbit) -> CamAnim {
        CamAnim { now: o, vel: [0.0; 6], goal: None }
    }

    /// Takes the camera over directly (a drag or wheel): any flight stops where it is.
    pub fn grab(&mut self) -> &mut Orbit {
        self.goal = None;
        self.vel = [0.0; 6];
        &mut self.now
    }

    pub fn fly(&mut self, mut to: Orbit) {
        let tau = std::f32::consts::TAU;
        to.yaw = self.now.yaw + (to.yaw - self.now.yaw + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
        self.goal = Some(to);
    }

    pub fn step(&mut self, dt: f32) -> bool {
        let Some(g) = self.goal else { return false };
        let n = &mut self.now;
        let cur = [n.target.x, n.target.y, n.target.z, n.yaw, n.pitch, n.dist.ln()];
        let to = [g.target.x, g.target.y, g.target.z, g.yaw, g.pitch, g.dist.ln()];
        let mut out = [0.0; 6];
        let mut done = true;
        for i in 0..6 {
            let (x, v) = FLY.sample(cur[i], self.vel[i], to[i], dt);
            out[i] = x;
            self.vel[i] = v;
            let scale = if i < 3 { g.dist * 1e-4 } else { 1e-4 };
            done &= (x - to[i]).abs() < scale && v.abs() < scale * 10.0;
        }
        *n = Orbit { target: Vec3::new(out[0], out[1], out[2]), yaw: out[3], pitch: out[4], dist: out[5].exp() };
        if done {
            self.now = g;
            self.goal = None;
            self.vel = [0.0; 6];
        }
        true
    }
}

/// Where a part is drawn and how opaque it is, `t` of the way from plate (0) to assembly (1),
/// with assembled parts pushed apart from `center` by `explode`.
pub fn pose(part: &Part, t: f32, explode: f32, center: Vec3) -> (Mat4, f32) {
    let smooth = |a: f32, b: f32, x: f32| {
        let k = ((x - a) / (b - a)).clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    };
    let c = part.mesh.center();
    match part.assembly {
        Some(a) if part.printed => {
            let (_, rot, _) = a.to_scale_rotation_translation();
            let at = a.transform_point3(c);
            let end = at + (at - center) * explode;
            // Parts arc over each other on the way instead of passing through.
            let lift = (t.clamp(0.0, 1.0) * std::f32::consts::PI).sin() * ((part.mesh.max - part.mesh.min).length() * 0.45 + 10.0);
            let mid = part.plate.transform_point3(c).lerp(end, t) + Vec3::Z * lift;
            let r = Quat::IDENTITY.slerp(rot, t.clamp(-0.2, 1.2));
            (Mat4::from_translation(mid) * Mat4::from_quat(r) * Mat4::from_translation(-c), 1.0)
        }
        Some(a) => {
            let at = a.transform_point3(c);
            (Mat4::from_translation((at - center) * explode) * a, GHOST * smooth(0.45, 1.0, t))
        }
        None => (part.plate, 1.0 - smooth(0.0, 0.55, t)),
    }
}

/// The world box of `parts` posed at `t`.
pub fn bounds(parts: &[&Part], t: f32, explode: f32, center: Vec3) -> Option<(Vec3, Vec3)> {
    let mut b: Option<(Vec3, Vec3)> = None;
    for p in parts {
        let (m, alpha) = pose(p, t, explode, center);
        if alpha < GHOST * 0.9 {
            continue;
        }
        let (lo, hi) = (p.mesh.min, p.mesh.max);
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let w = m.transform_point3(c);
            b = Some(b.map_or((w, w), |(a, z)| (a.min(w), z.max(w))));
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_projects_back_onto_its_own_ray() {
        let o = Orbit { target: Vec3::new(10.0, 20.0, 5.0), yaw: -0.8, pitch: 0.5, dist: 300.0 };
        let p = Vec3::new(40.0, -10.0, 30.0);
        let l = Lens { w: 800.0, h: 600.0, shift: 120.0 };
        let s = o.project(p, &l).unwrap();
        let (a, d) = o.ray(s.x, s.y, &l);
        let off = (p - a) - d * (p - a).dot(d);
        assert!(off.length() < 1e-2, "{off}");
    }

    #[test]
    fn the_target_lands_at_the_shifted_centre() {
        let o = Orbit { target: Vec3::new(1.0, 2.0, 3.0), yaw: 0.3, pitch: 0.2, dist: 100.0 };
        let s = o.project(o.target, &Lens { w: 800.0, h: 600.0, shift: 100.0 }).unwrap();
        assert!((s.x - 300.0).abs() < 1e-3 && (s.y - 300.0).abs() < 1e-3, "{s}");
    }

    #[test]
    fn the_iso_view_looks_from_front_right_above() {
        let e = Orbit::preset("iso", Vec3::ZERO, 50.0, 1.5).eye();
        assert!(e.x > 0.0 && e.y < 0.0 && e.z > 0.0, "{e}");
    }

    #[test]
    fn a_flight_lands_on_its_goal() {
        let mut c = CamAnim::new(Orbit { target: Vec3::ZERO, yaw: 3.0, pitch: 0.2, dist: 100.0 });
        let goal = Orbit { target: Vec3::new(5.0, 0.0, 0.0), yaw: -3.0, pitch: 0.4, dist: 50.0 };
        c.fly(goal);
        for _ in 0..240 {
            c.step(1.0 / 60.0);
        }
        assert!(c.goal.is_none());
        assert!((c.now.target - goal.target).length() < 1e-3 && (c.now.dist - 50.0).abs() < 1e-3);
        assert!(((c.now.yaw - goal.yaw).rem_euclid(std::f32::consts::TAU)).min(std::f32::consts::TAU - (c.now.yaw - goal.yaw).rem_euclid(std::f32::consts::TAU)) < 1e-3);
    }
}
