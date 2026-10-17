"""Offscreen PNG contact sheets of parts and assemblies."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pyvista as pv
import trimesh
from build123d import Keep, Plane, Shape

pv.OFF_SCREEN = True

BASE = np.array([200, 200, 205])
OVERHANG = np.array([230, 60, 50])
BRIDGE = np.array([245, 160, 40])
CAP = np.array([70, 130, 220])
PALETTE = [(90, 140, 220), (235, 150, 60), (110, 185, 110), (210, 90, 110), (160, 120, 200), (120, 190, 200)]

AXES = {"x": (1, 0, 0), "y": (0, 1, 0), "z": (0, 0, 1)}
# (title, camera direction from target, view-up, orthographic)
VIEWS = {
    "iso": ("iso", (1, -1, 0.8), (0, 0, 1), False),
    "iso_below": ("iso from below/back", (-1, 1, -0.7), (0, 0, 1), False),
    "front": ("front (-Y)", (0, -1, 0), (0, 0, 1), True),
    "right": ("right (+X)", (1, 0, 0), (0, 0, 1), True),
    "top": ("top (+Z)", (0, 0, 1), (0, 1, 0), True),
}


def tessellate(shape: Shape, tol: float = 0.01, angular: float = 0.1) -> trimesh.Trimesh:
    v, f = shape.tessellate(tol, angular)
    return trimesh.Trimesh(np.array([tuple(p) for p in v]), np.array(f), process=True)


def _poly(mesh: trimesh.Trimesh, colors: np.ndarray | None = None) -> pv.PolyData:
    faces = np.hstack([np.full((len(mesh.faces), 1), 3), mesh.faces]).ravel()
    pd = pv.PolyData(np.asarray(mesh.vertices), faces)
    if colors is not None:
        pd.cell_data["rgb"] = colors.astype(np.uint8)
    return pd


def _add(pl: pv.Plotter, pd: pv.PolyData, color=None) -> None:
    if color is None:
        pl.add_mesh(pd, scalars="rgb", rgb=True, show_scalar_bar=False, smooth_shading=False)
    else:
        pl.add_mesh(pd, color=color, smooth_shading=False)
    edges = pd.extract_feature_edges(feature_angle=35, boundary_edges=False, non_manifold_edges=False, manifold_edges=False)
    if edges.n_points:
        pl.add_mesh(edges, color="black", line_width=1.2)


def _camera(pl: pv.Plotter, bounds, direction, up, ortho: bool) -> None:
    lo, hi = np.array(bounds[0]), np.array(bounds[1])
    center = (lo + hi) / 2
    d = np.array(direction, float)
    d /= np.linalg.norm(d)
    dist = np.linalg.norm(hi - lo) * 3 + 10
    pl.camera_position = [tuple(center + d * dist), tuple(center), up]
    if ortho:
        pl.enable_parallel_projection()
    pl.reset_camera()
    pl.camera.zoom(0.95 if ortho else 1.0)


def _ortho_bounds(pl: pv.Plotter, view: str) -> None:
    hide = {"front": "y", "right": "x", "top": "z"}.get(view)
    kw = {f"show_{a}axis": a != hide for a in "xyz"}
    pl.show_bounds(grid=False, location="outer", ticks="outside", font_size=9, n_xlabels=3, n_ylabels=3, n_zlabels=3,
                   xtitle="X", ytitle="Y", ztitle="Z", fmt="%.0f", **kw)


def section(shape: Shape, axis: str, offset: float) -> tuple[trimesh.Trimesh, np.ndarray] | None:
    """Cut `shape` with a plane normal to `axis` at `offset`; keep the + side. Returns mesh and cap-face mask."""
    normal = AXES[axis]
    origin = tuple(offset * c for c in normal)
    half = shape.split(Plane(origin=origin, z_dir=normal), keep=Keep.TOP)
    if half is None or not getattr(half, "volume", 0):
        return None
    m = tessellate(half)
    i = "xyz".index(axis)
    cap = (m.face_normals[:, i] < -0.999) & (np.abs(m.triangles_center[:, i] - offset) < 1e-3)
    return m, cap


def _colors(n_faces: int, overhang=None, bridge=None) -> np.ndarray:
    c = np.tile(BASE, (n_faces, 1))
    if bridge is not None and len(bridge):
        c[bridge] = BRIDGE
    if overhang is not None and len(overhang):
        c[overhang] = OVERHANG
    return c


def part_sheet(name: str, shape: Shape, mesh: trimesh.Trimesh, out: Path, overhang=None, bridge=None,
               thin_points=None, section_spec: tuple[str, float] | None = None, header: str = "") -> Path:
    pl = pv.Plotter(shape=(2, 3), off_screen=True, window_size=(1800, 1150), border=True)
    pl.set_background("white")
    pd = _poly(mesh, _colors(len(mesh.faces), overhang, bridge))
    bounds = mesh.bounds
    for k, view in enumerate(["iso", "iso_below", "front", "right", "top"]):
        title, d, up, ortho = VIEWS[view]
        pl.subplot(k // 3, k % 3)
        _add(pl, pd.copy())
        if view == "iso" and thin_points is not None and len(thin_points):
            pl.add_points(np.asarray(thin_points), color="magenta", point_size=6, render_points_as_spheres=True)
        pl.add_text(title, font_size=9, color="black")
        if k == 0:
            pl.add_text(f"{name}  {header}", position="lower_left", font_size=9, color="black")
            pl.add_text("red: overhang  orange: flat ceiling  magenta: thin wall", position="upper_right", font_size=7, color="dimgray")
        _camera(pl, bounds, d, up, ortho)
        if ortho:
            _ortho_bounds(pl, view)
    pl.subplot(1, 2)
    axis, offset = section_spec or ("y", float(mesh.bounds.mean(axis=0)[1]))
    cut = section(shape, axis, offset)
    if cut:
        m, cap = cut
        c = np.tile(BASE, (len(m.faces), 1))
        c[cap] = CAP
        _add(pl, _poly(m, c))
        look = tuple(-x for x in AXES[axis])
        up = (0, 1, 0) if axis == "z" else (0, 0, 1)
        _camera(pl, m.bounds, look, up, True)
        _ortho_bounds(pl, {"x": "right", "y": "front", "z": "top"}[axis])
        pl.add_text(f"section {axis}={offset:.1f} (blue = cut)", font_size=9, color="black")
    else:
        pl.add_text(f"section {axis}={offset:.1f}: plane misses part", font_size=9, color="black")
    out.parent.mkdir(parents=True, exist_ok=True)
    pl.screenshot(str(out))
    pl.close()
    return out


def assembly_sheet(parts: dict[str, Shape], out: Path, section_spec: tuple[str, float] | None = None) -> Path:
    meshes = {n: tessellate(s) for n, s in parts.items()}
    allv = np.vstack([m.vertices for m in meshes.values()])
    bounds = np.array([allv.min(axis=0), allv.max(axis=0)])
    axis, offset = section_spec or ("y", float(bounds.mean(axis=0)[1]))
    pl = pv.Plotter(shape=(2, 2), off_screen=True, window_size=(1500, 1150), border=True)
    pl.set_background("white")
    panels = [("iso", *VIEWS["iso"][1:]), ("iso from below/back", *VIEWS["iso_below"][1:]), ("front (-Y)", *VIEWS["front"][1:])]
    for k, (title, d, up, ortho) in enumerate(panels):
        pl.subplot(k // 2, k % 2)
        for i, m in enumerate(meshes.values()):
            _add(pl, _poly(m), color=PALETTE[i % len(PALETTE)])
        pl.add_text(title, font_size=9, color="black")
        if k == 0:
            legend = "   ".join(meshes)
            pl.add_text(f"assembly: {legend}", position="lower_left", font_size=9, color="black")
        _camera(pl, bounds, d, up, ortho)
        if ortho:
            _ortho_bounds(pl, "front")
    pl.subplot(1, 1)
    any_cut = False
    for i, s in enumerate(parts.values()):
        cut = section(s, axis, offset)
        if cut:
            m, cap = cut
            col = np.array(PALETTE[i % len(PALETTE)])
            c = np.tile(col, (len(m.faces), 1))
            c[cap] = (col * 0.55).astype(int)
            _add(pl, _poly(m, c))
            any_cut = True
    if any_cut:
        look = tuple(-x for x in AXES[axis])
        up = (0, 1, 0) if axis == "z" else (0, 0, 1)
        _camera(pl, bounds, look, up, True)
        _ortho_bounds(pl, {"x": "right", "y": "front", "z": "top"}[axis])
    pl.add_text(f"section {axis}={offset:.1f} (dark = cut)", font_size=9, color="black")
    out.parent.mkdir(parents=True, exist_ok=True)
    pl.screenshot(str(out))
    pl.close()
    return out
