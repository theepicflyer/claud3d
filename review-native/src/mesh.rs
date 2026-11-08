//! Triangle meshes from STL, a BVH for ray casts, and crease edges for parts without B-rep topology.

use std::collections::HashMap;
use std::path::Path;

use glam::Vec3;

pub struct Mesh {
    pub tris: Vec<[Vec3; 3]>,
    pub normals: Vec<Vec3>,
    pub min: Vec3,
    pub max: Vec3,
    bvh: Vec<Node>,
    order: Vec<u32>,
}

#[derive(Clone, Copy)]
struct Node {
    min: Vec3,
    max: Vec3,
    /// Leaf: first index into `order`; inner: index of the left child (right is left + 1).
    start: u32,
    /// Leaf: triangle count; inner: 0.
    count: u32,
}

const LEAF: usize = 6;

impl Mesh {
    pub fn load_stl(path: &Path) -> Result<Mesh, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let tris = parse_stl(&bytes).ok_or_else(|| format!("{}: not a binary or ASCII STL", path.display()))?;
        Ok(Mesh::new(tris))
    }

    pub fn new(tris: Vec<[Vec3; 3]>) -> Mesh {
        let normals = tris.iter().map(|t| (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero()).collect();
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for t in &tris {
            for p in t {
                min = min.min(*p);
                max = max.max(*p);
            }
        }
        if tris.is_empty() {
            (min, max) = (Vec3::ZERO, Vec3::ZERO);
        }
        let mut m = Mesh { tris, normals, min, max, bvh: Vec::new(), order: Vec::new() };
        m.build_bvh();
        m
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    fn build_bvh(&mut self) {
        let n = self.tris.len();
        self.order = (0..n as u32).collect();
        let cent: Vec<Vec3> = self.tris.iter().map(|t| (t[0] + t[1] + t[2]) / 3.0).collect();
        self.bvh = vec![Node { min: Vec3::ZERO, max: Vec3::ZERO, start: 0, count: n as u32 }];
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let (start, count) = (self.bvh[i].start as usize, self.bvh[i].count as usize);
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            let (mut clo, mut chi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for &t in &self.order[start..start + count] {
                for p in &self.tris[t as usize] {
                    lo = lo.min(*p);
                    hi = hi.max(*p);
                }
                clo = clo.min(cent[t as usize]);
                chi = chi.max(cent[t as usize]);
            }
            self.bvh[i].min = lo;
            self.bvh[i].max = hi;
            if count <= LEAF {
                continue;
            }
            let ext = chi - clo;
            let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
            let slice = &mut self.order[start..start + count];
            let mid = count / 2;
            slice.select_nth_unstable_by(mid, |a, b| cent[*a as usize][axis].total_cmp(&cent[*b as usize][axis]));
            let left = self.bvh.len();
            self.bvh.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, start: start as u32, count: mid as u32 });
            self.bvh.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, start: (start + mid) as u32, count: (count - mid) as u32 });
            self.bvh[i].start = left as u32;
            self.bvh[i].count = 0;
            stack.push(left);
            stack.push(left + 1);
        }
    }

    /// Nearest hit along the ray `o + t d` with `t` in (0, max_t) that `keep` accepts: (t, triangle).
    pub fn raycast(&self, o: Vec3, d: Vec3, max_t: f32, keep: &dyn Fn(Vec3) -> bool) -> Option<(f32, usize)> {
        if self.tris.is_empty() {
            return None;
        }
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best: Option<(f32, usize)> = None;
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let nd = self.bvh[i];
            let limit = best.map_or(max_t, |b| b.0);
            if !slab(nd.min, nd.max, o, inv, limit) {
                continue;
            }
            if nd.count == 0 {
                stack.push(nd.start as usize);
                stack.push(nd.start as usize + 1);
                continue;
            }
            for &t in &self.order[nd.start as usize..(nd.start + nd.count) as usize] {
                if let Some(h) = tri_hit(&self.tris[t as usize], o, d)
                    && h > 1e-4
                    && h < best.map_or(max_t, |b| b.0)
                    && keep(o + d * h)
                {
                    best = Some((h, t as usize));
                }
            }
        }
        best
    }

    /// Sharp edges (dihedral angle above `deg`) and boundary edges, for meshes without B-rep edges.
    pub fn crease_edges(&self, deg: f32) -> Vec<[Vec3; 2]> {
        let q = |p: Vec3| ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64, (p.z * 1e4).round() as i64);
        let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut pts: Vec<Vec3> = Vec::new();
        let mut id = |p: Vec3| *ids.entry(q(p)).or_insert_with(|| {
            pts.push(p);
            pts.len() as u32 - 1
        });
        let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (ti, t) in self.tris.iter().enumerate() {
            let v = [id(t[0]), id(t[1]), id(t[2])];
            for k in 0..3 {
                let (a, b) = (v[k], v[(k + 1) % 3]);
                if a != b {
                    edges.entry((a.min(b), a.max(b))).or_default().push(ti);
                }
            }
        }
        let cos = deg.to_radians().cos();
        edges
            .into_iter()
            .filter(|(_, f)| f.len() != 2 || self.normals[f[0]].dot(self.normals[f[1]]) < cos)
            .map(|((a, b), _)| [pts[a as usize], pts[b as usize]])
            .collect()
    }
}

fn slab(min: Vec3, max: Vec3, o: Vec3, inv: Vec3, limit: f32) -> bool {
    let t1 = (min - o) * inv;
    let t2 = (max - o) * inv;
    let tmin = t1.min(t2).max_element().max(0.0);
    let tmax = t1.max(t2).min_element().min(limit);
    tmin <= tmax
}

fn tri_hit(t: &[Vec3; 3], o: Vec3, d: Vec3) -> Option<f32> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qv = s.cross(e1);
    let v = d.dot(qv) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(e2.dot(qv) * inv)
}

fn parse_stl(b: &[u8]) -> Option<Vec<[Vec3; 3]>> {
    if b.len() >= 84 {
        let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
        if b.len() == 84 + n * 50 {
            let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
            let v = |o: usize| Vec3::new(f(o), f(o + 4), f(o + 8));
            return Some((0..n).map(|i| 84 + i * 50).map(|o| [v(o + 12), v(o + 24), v(o + 36)]).collect());
        }
    }
    let text = std::str::from_utf8(b).ok()?;
    if !text.trim_start().starts_with("solid") {
        return None;
    }
    let mut verts = Vec::new();
    for line in text.lines() {
        let mut w = line.split_whitespace();
        if w.next() == Some("vertex") {
            let c: Vec<f32> = w.filter_map(|x| x.parse().ok()).collect();
            if c.len() == 3 {
                verts.push(Vec3::new(c[0], c[1], c[2]));
            }
        }
    }
    Some(verts.as_chunks::<3>().0.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Mesh {
        let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        let quads = [
            [p(0., 0., 0.), p(0., 1., 0.), p(1., 1., 0.), p(1., 0., 0.)],
            [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)],
            [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)],
            [p(0., 1., 0.), p(0., 1., 1.), p(1., 1., 1.), p(1., 1., 0.)],
            [p(0., 0., 0.), p(0., 0., 1.), p(0., 1., 1.), p(0., 1., 0.)],
            [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)],
        ];
        Mesh::new(quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect())
    }

    #[test]
    fn a_ray_hits_the_nearest_face_and_respects_the_filter() {
        let m = cube();
        let (t, tri) = m.raycast(Vec3::new(0.5, 0.5, 5.0), Vec3::NEG_Z, f32::MAX, &|_| true).unwrap();
        assert!((t - 4.0).abs() < 1e-5);
        assert!(m.normals[tri].z > 0.99, "the top face, facing up");
        let (t, _) = m.raycast(Vec3::new(0.5, 0.5, 5.0), Vec3::NEG_Z, f32::MAX, &|p| p.z < 0.5).unwrap();
        assert!((t - 5.0).abs() < 1e-5, "the filter skips the top and finds the bottom");
        assert!(m.raycast(Vec3::new(2.0, 0.5, 5.0), Vec3::NEG_Z, f32::MAX, &|_| true).is_none());
    }

    #[test]
    fn a_cube_has_twelve_crease_edges() {
        assert_eq!(cube().crease_edges(30.0).len(), 12);
    }

    #[test]
    fn binary_stl_round_trips() {
        let mut b = vec![0u8; 80];
        b.extend(1u32.to_le_bytes());
        for v in [[0f32, 0., 0.], [0., 0., 1.], [1., 0., 0.], [0., 1., 0.]] {
            for c in v {
                b.extend(c.to_le_bytes());
            }
        }
        b.extend([0u8, 0]);
        let m = Mesh::new(parse_stl(&b).unwrap());
        assert_eq!(m.tris.len(), 1);
        assert_eq!((m.min, m.max), (Vec3::ZERO, Vec3::ONE));
    }
}
