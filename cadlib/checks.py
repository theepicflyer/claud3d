"""Printability checks on tessellated parts."""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np
import trimesh
from scipy.spatial import cKDTree

from .config import Settings

PASS, WARN, FAIL = "PASS", "WARN", "FAIL"
_RANK = {PASS: 0, WARN: 1, FAIL: 2}


@dataclass
class Finding:
    status: str
    check: str
    detail: str


@dataclass
class Region:
    area: float
    z_min: float
    z_max: float
    center: tuple[float, float, float]
    kind: str = ""       # ceilings: "bridge", "cantilever" or "floating" (no support below any edge)
    reach: float = 0.0   # farthest distance (XY) from any point of the region to a supported edge


@dataclass
class PartReport:
    name: str
    findings: list[Finding] = field(default_factory=list)
    stats: dict = field(default_factory=dict)
    overhang_faces: np.ndarray | None = None
    bridge_faces: np.ndarray | None = None
    thin_points: np.ndarray | None = None

    @property
    def status(self) -> str:
        return max((f.status for f in self.findings), key=_RANK.__getitem__, default=PASS)

    def add(self, status: str, check: str, detail: str) -> None:
        self.findings.append(Finding(status, check, detail))


def _groups(mesh: trimesh.Trimesh, face_ids: np.ndarray) -> list[np.ndarray]:
    if len(face_ids) == 0:
        return []
    adj = mesh.face_adjacency
    keep = np.isin(adj, face_ids).all(axis=1)
    return [np.asarray(g) for g in trimesh.graph.connected_components(adj[keep], nodes=face_ids, min_len=1)]


def _region(mesh: trimesh.Trimesh, g: np.ndarray) -> Region:
    verts = mesh.vertices[mesh.faces[g].ravel()]
    c = tuple(round(float(x), 1) for x in verts.mean(axis=0))
    return Region(float(mesh.area_faces[g].sum()), float(verts[:, 2].min()), float(verts[:, 2].max()), c)


def _ceiling(mesh: trimesh.Trimesh, g: np.ndarray, step: float = 0.5) -> Region:
    """Classify a flat downward face group by how its boundary is supported by walls below it."""
    r = _region(mesh, g)
    adj, adj_edges = mesh.face_adjacency, mesh.face_adjacency_edges
    inside = np.isin(adj, g)
    boundary = inside.sum(axis=1) == 1
    support = []
    for (fa, fb), edge, ina in zip(adj[boundary], adj_edges[boundary], inside[boundary][:, 0], strict=True):
        other = fb if ina else fa
        a, b = mesh.vertices[edge]
        if mesh.triangles_center[other][2] < min(a[2], b[2]) - 1e-4:
            n = max(int(np.linalg.norm(b - a) / step), 1)
            support.append(a[:2] + np.outer(np.linspace(0, 1, n + 1), b[:2] - a[:2]))
    sub = mesh.submesh([g], append=True)
    pts = trimesh.sample.sample_surface_even(sub, int(np.clip(r.area / 0.25, 50, 3000)), seed=0)[0][:, :2]
    pts = np.vstack([pts, sub.vertices[:, :2]])
    if not support:
        r.kind = "floating"
        return r
    sp = np.vstack(support)
    tree = cKDTree(sp)
    dist, idx = tree.query(pts)
    far = int(np.argmax(dist))
    r.reach = float(dist[far])
    to_near = sp[idx[far]] - pts[far]
    opposed = ((sp - pts[far]) @ to_near < -0.5 * np.dot(to_near, to_near)).any()
    r.kind = "bridge" if opposed or r.reach < 1e-6 else "cantilever"
    return r


def wall_thickness(mesh: trimesh.Trimesh, samples: int = 4000, seed: int = 0) -> tuple[np.ndarray, np.ndarray]:
    """Ray-cast inward from surface samples to the opposite wall. Returns (thickness, points)."""
    pts, fidx = trimesh.sample.sample_surface_even(mesh, samples, seed=seed)
    if len(pts) == 0:
        return np.array([]), np.empty((0, 3))
    normals = mesh.face_normals[fidx]
    origins = pts - normals * 1e-3
    dirs = -normals
    locs, ray_idx, tri_idx = mesh.ray.intersects_location(origins, dirs, multiple_hits=False)
    if len(ray_idx) == 0:
        return np.array([]), np.empty((0, 3))
    # Only count hits on roughly opposing faces; grazing hits near edges are not real walls.
    facing = (mesh.face_normals[tri_idx] * dirs[ray_idx]).sum(axis=1) > 0.7
    d = np.linalg.norm(locs - origins[ray_idx], axis=1)[facing]
    return d, pts[ray_idx][facing]


def check_part(name: str, mesh: trimesh.Trimesh, s: Settings) -> PartReport:
    r = PartReport(name)
    d = s.design
    lo, hi = mesh.bounds
    size = hi - lo
    r.stats.update(
        bbox=[round(float(x), 2) for x in size],
        bounds_min=[round(float(x), 2) for x in lo],
        volume_cm3=round(float(mesh.volume) / 1000, 2),
        area_cm2=round(float(mesh.area) / 100, 2),
        triangles=len(mesh.faces),
    )

    if mesh.is_watertight and mesh.is_winding_consistent and mesh.volume > 0:
        r.add(PASS, "manifold", "watertight, consistent winding")
    else:
        r.add(FAIL, "manifold", f"watertight={mesh.is_watertight} winding={mesh.is_winding_consistent} volume={mesh.volume:.1f}")

    bodies = mesh.split(only_watertight=False)
    if len(bodies) > 1:
        r.add(WARN, "bodies", f"{len(bodies)} disconnected bodies (intended? floating parts print as separate objects)")
    r.stats["bodies"] = len(bodies)

    bv = np.array(s.build_volume)
    if (size <= bv).all():
        r.add(PASS, "build volume", f"{size[0]:.1f} x {size[1]:.1f} x {size[2]:.1f} fits {bv[0]:.0f} x {bv[1]:.0f} x {bv[2]:.0f}")
    else:
        r.add(FAIL, "build volume", f"{size[0]:.1f} x {size[1]:.1f} x {size[2]:.1f} exceeds {bv[0]:.0f} x {bv[1]:.0f} x {bv[2]:.0f}")

    n = mesh.face_normals
    cz = mesh.triangles_center[:, 2]
    on_bed = (n[:, 2] < -0.999) & (cz < lo[2] + 1e-3)
    contact = float(mesh.area_faces[on_bed].sum())
    r.stats["bed_contact_mm2"] = round(contact, 1)
    if abs(lo[2]) > 1e-3:
        r.add(FAIL, "on bed", f"lowest point is z={lo[2]:.3f}, expected 0")
    elif contact < d["min_bed_contact"]:
        r.add(WARN, "bed contact", f"only {contact:.1f} mm^2 flat on the bed; consider a brim or reorienting")
    else:
        r.add(PASS, "bed contact", f"{contact:.0f} mm^2")

    limit = np.sin(np.radians(90 - d["overhang_angle"]))
    downward = (n[:, 2] < -limit - 1e-3) & ~on_bed  # tolerance lets exact 45 deg chamfers pass
    bridges = np.where(downward & (n[:, 2] < -0.999))[0]
    overhangs = np.where(downward & (n[:, 2] >= -0.999))[0]
    r.overhang_faces, r.bridge_faces = overhangs, bridges

    oh_regions = [x for x in map(lambda g: _region(mesh, g), _groups(mesh, overhangs)) if x.area > 1.0]
    oh_regions.sort(key=lambda x: -x.area)
    if oh_regions:
        desc = "; ".join(f"{x.area:.0f} mm^2 at z={x.z_min:.1f}-{x.z_max:.1f} near {x.center}" for x in oh_regions[:4])
        r.add(WARN, "overhangs", f"{len(oh_regions)} region(s) steeper than {d['overhang_angle']} deg: {desc}")
    else:
        r.add(PASS, "overhangs", f"none steeper than {d['overhang_angle']} deg")

    ceilings = [x for x in map(lambda g: _ceiling(mesh, g), _groups(mesh, bridges)) if x.area > 1.0]
    bad = []
    for x in ceilings:
        where = f"{x.area:.0f} mm^2 at z={x.z_min:.1f} near {x.center}"
        if x.kind == "floating":
            bad.append(f"floating ceiling with no wall below ({where}), needs support")
        elif x.kind == "cantilever" and x.reach > d["max_cantilever"]:
            bad.append(f"cantilevered ceiling reaching {x.reach:.1f} mm past its support ({where})")
        elif x.kind == "bridge" and 2 * x.reach > d["max_bridge"]:
            bad.append(f"bridge spanning ~{2 * x.reach:.0f} mm ({where})")
    if bad:
        r.add(WARN, "bridges", f"{len(bad)} flat ceiling(s) need attention: " + "; ".join(bad[:4]))
    elif ceilings:
        widest = max((2 * x.reach if x.kind == "bridge" else x.reach) for x in ceilings)
        r.add(PASS, "bridges", f"{len(ceilings)} flat ceiling(s), longest unsupported ~{widest:.1f} mm")
    else:
        r.add(PASS, "bridges", "no flat ceilings")

    t, pts = wall_thickness(mesh)
    if len(t):
        p1 = float(np.percentile(t, 1))
        tmin = float(t.min())
        r.stats["wall_min_mm"] = round(tmin, 2)
        r.stats["wall_p1_mm"] = round(p1, 2)
        thin = t < d["min_wall"]
        r.thin_points = pts[thin]
        frac = thin.mean() * 100
        where = ""
        if thin.any():
            c = tuple(round(float(x), 1) for x in pts[thin][np.argmin(t[thin])])
            where = f", thinnest {tmin:.2f} mm near {c}"
        if tmin < d["min_wall_hard"] and frac > 0.2:
            r.add(FAIL, "wall thickness", f"{frac:.1f}% of samples below {d['min_wall']} mm{where}")
        elif frac > 0.2:
            r.add(WARN, "wall thickness", f"{frac:.1f}% of samples below {d['min_wall']} mm{where}")
        else:
            r.add(PASS, "wall thickness", f"min ~{p1:.2f} mm (1st percentile)")
    return r


def check_interference(parts: dict, min_clearance: float = 0.0) -> list[Finding]:
    """Pairwise B-rep intersection between assembled parts."""
    names = list(parts)
    out = []
    for i, a in enumerate(names):
        for b in names[i + 1:]:
            pa, pb = parts[a], parts[b]
            common = pa & pb
            vol = sum(s.volume for s in common.solids()) if common is not None else 0.0
            if vol > 1e-3:
                out.append(Finding(FAIL, "interference", f"{a} and {b} overlap by {vol:.2f} mm^3"))
            else:
                gap = pa.distance_to(pb)
                status = WARN if gap < min_clearance else PASS
                out.append(Finding(status, "clearance", f"{a} / {b}: min gap {gap:.3f} mm"))
    return out
