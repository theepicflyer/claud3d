//! A project's build output (`out/view.json`, part STLs and topology) and its review threads.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use glam::{Mat4, Vec3, Vec4};
use serde_json::Value;

use crate::mesh::Mesh;

/// Part colours, as the web review app assigns them in view.json order.
pub const PALETTE: [u32; 6] = [0x5a8cdc, 0xeb963c, 0x6eb96e, 0xd25a6e, 0xa078c8, 0x78bec8];
/// Assembly-only parts (boards, counters, mating hardware) are context, not prints.
pub const FIXTURE: u32 = 0xb8b2a6;

pub fn rgb(c: u32) -> [f32; 3] {
    let lin = |v: u32| rune::color::srgb_decode((v & 0xff) as f32 / 255.0);
    [lin(c >> 16), lin(c >> 8), lin(c)]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapKind {
    Vertex,
    Center,
    Midpoint,
}

impl SnapKind {
    pub fn name(self) -> &'static str {
        match self {
            SnapKind::Vertex => "vertex",
            SnapKind::Center => "center",
            SnapKind::Midpoint => "midpoint",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snap {
    pub kind: SnapKind,
    pub p: Vec3,
    /// Circle radius, for centres.
    pub r: Option<f32>,
}

pub struct Part {
    pub name: String,
    /// Unique per drawn object: the part's name, or `name@assembly` for an assembly-only mesh.
    pub key: String,
    /// Printed parts lie on the plate; fixtures only appear assembled.
    pub printed: bool,
    pub mesh: Mesh,
    pub edges: Vec<[Vec3; 2]>,
    pub snaps: Vec<Snap>,
    pub color: [f32; 3],
    pub plate: Mat4,
    pub assembly: Option<Mat4>,
    pub status: String,
    pub bbox: [f32; 3],
}

pub struct Nozzle {
    pub x: [f32; 2],
    pub y: [f32; 2],
}

pub struct Feature {
    pub name: String,
    pub part: String,
    pub point: Vec3,
    pub normal: Option<Vec3>,
    pub text: String,
    pub params: Vec<(String, String)>,
    pub view: Option<String>,
}

pub struct Finding {
    pub status: String,
    pub check: String,
    pub detail: String,
    pub items: Vec<Item>,
}

pub struct Item {
    pub text: String,
    pub at: Option<Vec3>,
    pub where_: Option<String>,
    pub gap: Option<f32>,
}

pub struct Slice {
    pub ok: bool,
    pub message: String,
    pub time: Option<String>,
    pub grams: Option<f64>,
    pub nozzle: Option<String>,
    pub material: Option<String>,
    pub plates: Option<u32>,
    pub skipped: bool,
}

pub struct PartReport {
    pub name: String,
    pub status: String,
    pub findings: Vec<Finding>,
}

pub struct Report {
    pub parts: Vec<PartReport>,
    pub assembly: Vec<Finding>,
    pub spec: Vec<Finding>,
    pub slice: Option<Slice>,
}

pub struct Scene {
    pub generation: u64,
    pub project: String,
    pub built_at: String,
    pub commit: String,
    pub dirty: bool,
    pub status: String,
    pub parts: Vec<Part>,
    pub nozzles: Vec<Nozzle>,
    pub features: Vec<Feature>,
    pub params: Vec<(String, String)>,
    pub report: Report,
    pub has_assembly: bool,
}

pub fn view_path(project: &Path) -> PathBuf {
    project.join("out").join("view.json")
}

pub fn comments_path(project: &Path) -> PathBuf {
    project.join("review").join("comments.json")
}

pub fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

fn vec3(v: &Value) -> Option<Vec3> {
    let a = v.as_array()?;
    (a.len() == 3).then(|| Vec3::new(f(&a[0]), f(&a[1]), f(&a[2])))
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn fmt_value(v: &Value) -> String {
    match v {
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or(0.0);
            if x.fract() == 0.0 && x.abs() < 1e9 { format!("{x:.0}") } else { format!("{}", (x * 1000.0).round() / 1000.0) }
        }
        other => s(other),
    }
}

fn mat4(v: &Value) -> Option<Mat4> {
    let rows = v.as_array()?;
    if rows.len() != 4 {
        return None;
    }
    let mut c = [Vec4::ZERO; 4];
    for (r, row) in rows.iter().enumerate() {
        let row = row.as_array()?;
        for (k, x) in row.iter().enumerate().take(4) {
            c[k][r] = f(x);
        }
    }
    Some(Mat4::from_cols(c[0], c[1], c[2], c[3]))
}

fn findings(v: &Value) -> Vec<Finding> {
    v.as_array().map_or_else(Vec::new, |a| {
        a.iter()
            .map(|x| Finding {
                status: s(&x["status"]),
                check: s(&x["check"]),
                detail: s(&x["detail"]),
                items: x["items"].as_array().map_or_else(Vec::new, |items| {
                    items
                        .iter()
                        .map(|it| Item {
                            text: s(&it["text"]),
                            at: vec3(&it["at"]),
                            where_: it["where"].as_str().map(str::to_owned),
                            gap: it["gap"].as_f64().map(|g| g as f32),
                        })
                        .collect()
                }),
            })
            .collect()
    })
}

fn topology(path: &Path) -> Option<(Vec<[Vec3; 2]>, Vec<Snap>)> {
    let t: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let mut segs = Vec::new();
    let mut snaps: Vec<Snap> = t["vertices"]
        .as_array()?
        .iter()
        .filter_map(vec3)
        .map(|p| Snap { kind: SnapKind::Vertex, p, r: None })
        .collect();
    for e in t["edges"].as_array()? {
        match e["t"].as_str() {
            Some("line") => {
                let (Some(a), Some(b)) = (vec3(&e["a"]), vec3(&e["b"])) else { continue };
                segs.push([a, b]);
                snaps.push(Snap { kind: SnapKind::Midpoint, p: (a + b) * 0.5, r: None });
            }
            _ => {
                let pts: Vec<Vec3> = e["pts"].as_array().map_or_else(Vec::new, |p| p.iter().filter_map(vec3).collect());
                segs.extend(pts.windows(2).map(|w| [w[0], w[1]]));
                if e["t"].as_str() == Some("circle")
                    && let Some(c) = vec3(&e["c"])
                {
                    snaps.push(Snap { kind: SnapKind::Center, p: c, r: e["r"].as_f64().map(|r| r as f32) });
                }
            }
        }
    }
    Some((segs, snaps))
}

pub fn load(project: &Path, generation: u64) -> Result<Scene, String> {
    let out = project.join("out");
    let raw = std::fs::read(view_path(project)).map_err(|e| format!("no build yet ({e}); run `uv run cad build`"))?;
    let v: Value = serde_json::from_slice(&raw).map_err(|e| format!("view.json: {e}"))?;
    let rep = &v["report"];
    let mut parts = Vec::new();
    let empty = serde_json::Map::new();
    let asm = v["assembly"].as_object().unwrap_or(&empty);
    for (i, (name, info)) in v["parts"].as_object().unwrap_or(&empty).iter().enumerate() {
        let mesh = Mesh::load_stl(&out.join(s(&info["stl"])))?;
        let (edges, snaps) = info["topo"]
            .as_str()
            .and_then(|t| topology(&out.join(t)))
            .unwrap_or_else(|| (mesh.crease_edges(30.0), Vec::new()));
        let off = vec3(&info["plate_offset"]).unwrap_or(Vec3::ZERO);
        parts.push(Part {
            name: name.clone(),
            key: name.clone(),
            printed: true,
            edges,
            snaps,
            color: rgb(PALETTE[i % PALETTE.len()]),
            plate: Mat4::from_translation(off),
            assembly: asm.get(name).and_then(|a| mat4(&a["matrix"])),
            status: s(&rep["parts"][name]["status"]),
            bbox: vec3(&info["bbox"]).map_or([0.0; 3], |b| b.to_array()),
            mesh,
        });
    }
    for (name, a) in asm {
        let Some(stl) = a["stl"].as_str() else { continue };
        let mesh = Mesh::load_stl(&out.join(stl))?;
        let color = parts.iter().find(|p| &p.name == name).map_or(rgb(FIXTURE), |p| p.color);
        let ext = mesh.max - mesh.min;
        parts.push(Part {
            name: name.clone(),
            key: format!("{name}@assembly"),
            printed: false,
            edges: mesh.crease_edges(30.0),
            snaps: Vec::new(),
            color,
            plate: Mat4::IDENTITY,
            assembly: Some(Mat4::IDENTITY),
            status: String::new(),
            bbox: ext.to_array(),
            mesh,
        });
    }
    let nozzles = v["plate"]["nozzles"].as_array().map_or_else(Vec::new, |a| {
        a.iter()
            .map(|n| Nozzle {
                x: [f(&n["x"][0]), f(&n["x"][1])],
                y: [f(&n["y"][0]), f(&n["y"][1])],
            })
            .collect()
    });
    let features = v["features"].as_array().map_or_else(Vec::new, |a| {
        a.iter()
            .filter_map(|x| {
                Some(Feature {
                    name: s(&x["name"]),
                    part: s(&x["part"]),
                    point: vec3(&x["point"])?,
                    normal: vec3(&x["normal"]),
                    text: s(&x["text"]),
                    params: x["params"].as_object().map_or_else(Vec::new, |m| m.iter().map(|(k, v)| (k.clone(), fmt_value(v))).collect()),
                    view: x["view"].as_str().map(str::to_owned),
                })
            })
            .collect()
    });
    let sl = &rep["slice"];
    let slice = (!sl.is_null()).then(|| Slice {
        ok: sl["ok"].as_bool().unwrap_or(false),
        message: s(&sl["message"]),
        time: sl["time"].as_str().map(str::to_owned),
        grams: sl["filament_g"].as_f64(),
        nozzle: sl["nozzle"].as_str().map(str::to_owned),
        material: sl["material"].as_str().map(str::to_owned),
        plates: sl["plates"].as_u64().map(|p| p as u32),
        skipped: sl["skipped"].as_bool().unwrap_or(false),
    });
    let report = Report {
        parts: rep["parts"].as_object().map_or_else(Vec::new, |m| {
            m.iter().map(|(n, r)| PartReport { name: n.clone(), status: s(&r["status"]), findings: findings(&r["findings"]) }).collect()
        }),
        assembly: findings(&rep["assembly"]),
        spec: findings(&rep["spec"]),
        slice,
    };
    Ok(Scene {
        generation,
        project: v["project"].as_str().map_or_else(|| project.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()), str::to_owned),
        built_at: s(&v["built_at"]),
        commit: s(&v["commit"]),
        dirty: v["dirty"].as_bool().unwrap_or(false),
        status: s(&v["status"]),
        has_assembly: !asm.is_empty(),
        parts,
        nozzles,
        features,
        params: v["params"].as_object().map_or_else(Vec::new, |m| m.iter().map(|(k, v)| (k.clone(), fmt_value(v))).collect()),
        report,
    })
}

/// A review thread as `cadlib/review` stores it. The anchor stays JSON so it round-trips untouched.
#[derive(Clone)]
pub struct Thread {
    pub id: i64,
    pub status: String,
    pub anchor: Value,
    pub camera: Value,
    pub screenshot: Option<PathBuf>,
    pub messages: Vec<(String, String)>,
}

pub fn load_threads(project: &Path) -> Vec<Thread> {
    let Ok(raw) = std::fs::read(comments_path(project)) else { return Vec::new() };
    let Ok(v) = serde_json::from_slice::<Value>(&raw) else { return Vec::new() };
    v["threads"].as_array().map_or_else(Vec::new, |a| {
        a.iter()
            .map(|t| Thread {
                id: t["id"].as_i64().unwrap_or(0),
                status: s(&t["status"]),
                anchor: t["anchor"].clone(),
                camera: t["camera"].clone(),
                screenshot: t["screenshot"].as_str().map(|p| project.join("review").join(p)),
                messages: t["messages"].as_array().map_or_else(Vec::new, |m| m.iter().map(|m| (s(&m["by"]), s(&m["text"]))).collect()),
            })
            .collect()
    })
}

pub fn anchor_vec(a: &Value, key: &str) -> Option<Vec3> {
    vec3(&a[key])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_are_read_row_major() {
        let m = mat4(&serde_json::json!([[1, 0, 0, 5], [0, 1, 0, 6], [0, 0, 1, 7], [0, 0, 0, 1]])).unwrap();
        assert_eq!(m.transform_point3(Vec3::ZERO), Vec3::new(5.0, 6.0, 7.0));
    }

    #[test]
    fn numbers_format_without_float_noise() {
        assert_eq!(fmt_value(&serde_json::json!(2.0)), "2");
        assert_eq!(fmt_value(&serde_json::json!(0.30000000000000004)), "0.3");
        assert_eq!(fmt_value(&serde_json::json!("snug")), "snug");
    }
}
