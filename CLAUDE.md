# claude-3d

Code-driven CAD for FDM parts printed on a Bambu Lab X2D. Each part is a parametric build123d script; one command exports it, checks printability, renders views for review, and slices it with Bambu Studio.

## Layout

```
printer.toml          printer, slicer presets, design limits, fit clearances, materials
cadlib/               shared library + `cad` CLI
  project.py          loads model.py, runs the build pipeline, writes the report
  checks.py           mesh checks: manifold, build volume, bed contact, overhangs, ceilings, wall thickness, interference
  render.py           offscreen PNG contact sheets (pyvista)
  bambu.py            Bambu Studio preset flattening, CLI slicing, opening projects
  features.py         FDM helpers: screw/nut tables, teardrop, nut trap, elephant-foot chamfer
  review/             browser review app (server.py + static/ with vendored three.js r170) and comment store
  template/           copied by `cad new`
projects/<name>/
  CLAUDE.md           that project's requirements, interfaces, print notes, decisions, print log
  model.py            the parametric model
  out/                generated (gitignored): STEP, STL, PNG renders, report.md/json, view.json, slice/<name>.gcode.3mf
  review/             committed: comments.json (review threads) and shots/<id>.png
tests/                pytest for checks and the example build
```

## Commands

```
uv run cad new <name>                         scaffold projects/<name> (lowercase-kebab-case)
uv run cad build <name> [-p key=value ...]    build + export + check + render + slice; exits 1 on FAIL
    --no-slice --no-render --open
uv run cad open <name> [--step]               open the sliced 3MF in Bambu Studio (or STEP in FreeCAD)
uv run cad review <name> [--no-open] [--keep]  3D review app in the browser; exits printing the comments on Send
uv run cad comments <name> [--all]            print open review threads (all with --all)
uv run cad reply <name> <id> "text" [--resolve|--reopen]
uv run cad profiles machine|process|filament [regex]
uv run cad list
uv run pytest -q
```

## Workflow for a new part

1. **Requirements first.** Before modeling, pin down: what the part does, what it mates with (get measured dimensions, ideally with calipers, and how they were measured), loads and environment (heat, outdoors/UV, flex, wear), material, quantity, cosmetic faces, and any size or print-time limits. Ask in small batches. Write them into the project CLAUDE.md as numbered requirements (R1, R2...) with measurable acceptance criteria. Call out ambiguity rather than guessing; when you must assume, record it under Decisions.
2. **Scaffold:** `uv run cad new <name>`.
3. **Choose print orientation before modeling.** It decides layer-line strength, which faces need support, and which faces are cosmetic. Model each part in its print orientation (flat face down at z=0). Use `assembly()` to place parts in their assembled positions.
4. **Model** in `model.py`. Every dimension that comes from a requirement or a mating part goes in `PARAMS`. Clearances come from `s.clearance("press"|"snug"|"slide"|"loose")`, which already includes the material's extra, never from hardcoded numbers.
5. **Write `check()`** to verify each requirement by measuring the built geometry (bounding boxes, sections, distances), not by echoing parameters.
6. **Build and review:** `uv run cad build <name>`. Read `out/report.md`, then **look at every PNG** in `out/` with the Read tool. Check the renders against the requirements: proportions, features present and in the right place, nothing floating, section shows the intended walls. The checks catch geometry problems; only looking catches design mistakes.
7. **Iterate** until the report is PASS or every remaining WARN is understood and recorded in the project's Decisions section (e.g. an intentional overhang that will get support).
8. **Hand off:** walk the user through the renders, report, slice time/filament, and decisions. Start the review app (see below) so they can inspect and comment. `uv run cad open <name>` opens the slice in Bambu Studio. Commit and open a PR (see Git).
9. **After printing:** record results in the project's Print log. If fits are consistently tight or loose across projects, adjust `[fit]` or the material's `clearance_extra` in printer.toml rather than patching individual models.

## Review loop

The user reviews models in the browser and comments on them; comments come back to Claude without copy-paste.

1. After a build, run `uv run cad review <name>` as a **background** Bash command. It serves on a fixed per-project port and opens a browser tab unless an existing tab reconnects within 4 s. Tell the user it's open.
2. The user can rotate/pan/zoom, switch between the plate layout and the assembly, cut a section, pin points, circle areas, measure, and write comments or replies. "Send to Claude" saves them and the server exits, which re-invokes Claude with the printed comments.
3. Each comment prints with its id, part, anchor in the **part's model frame** (the print-oriented frame from `build()` after the automatic drop to z=0, in mm, before plate placement), surface normal, the view it was made in, and the build it was made against. Read every screenshot path listed: it shows the user's viewpoint with the marker labelled "this" (and the circle for circled areas). Map coordinates back to features in `model.py` before changing anything; ask if a comment is ambiguous.
4. Make the changes, rebuild, then answer each thread with `uv run cad reply <name> <id> "what changed" --resolve` (or without `--resolve` when asking a question back). Restart the review in the background; the user's open tab reconnects and shows the replies. `cad review --keep` keeps serving after a send, e.g. while rebuilding repeatedly; the tab reloads the model whenever `out/view.json` changes.
5. `uv run cad comments <name>` lists open threads at any time, including from earlier sessions. Commit `review/` with the project.

## model.py contract

```python
PARAMS = dict(...)                 # required; exposed as p.<name>; CLI overrides with -p
PRINT = dict(material="pla")       # optional: material, process, filament, machine, slicer={...}, design={...}, fit={...}
SECTIONS = {"part": ("y", 0.0)}    # optional: section plane per part or "assembly"; default is mid-Y
def build(p, s) -> dict[str, Part] # required; print orientation; each part is dropped to z=0 automatically
def assembly(p, s, parts) -> dict  # optional; assembled positions -> assembly.png + interference check
def check(p, s, parts)             # optional; yield (ok: bool, "R1 message") per requirement
```

`s` is `cadlib.config.Settings`: `s.clearance(kind)`, `s.nozzle`, `s.layer_height`, `s.material.key`, `s.design[...]`, `s.nozzles` (per-nozzle regions), `s.shared` (region every nozzle reaches), `s.build_volume` (largest single-nozzle volume). All parts of a project are sliced together on one plate with the project's material.

## Reading the report

- **build volume**: PASS if the part fits the region both nozzles reach (235.5 x 256 x 256). WARN if it only fits after a 90 deg rotation about Z, or only fits the left nozzle (256 x 256 x 261). FAIL if no nozzle can print it. See "X2D nozzles" below.
- **manifold** FAIL: the solid is broken; usually a bad boolean or a zero-thickness feature. Fix before anything else.
- **bodies** WARN: several disconnected solids in one part. Fine for intentional multi-object plates, otherwise a feature didn't fuse (coincident faces; overlap booleans by 0.01 mm).
- **overhangs** (red in renders): downward faces steeper than 45 deg from vertical. Fix by reorienting, chamfering instead of filleting, or 45 deg transitions. A 45 deg chamfer sits exactly at the limit and passes.
- **bridges** (orange): flat downward faces. `bridge` = supported on opposite sides, warns past 15 mm. `cantilever` = shelf supported on one side, warns past 2 mm. `floating` = nothing below any edge, needs support.
- **wall thickness** (magenta dots): ray-cast thickness. Expect hits at chamfer tips and sharp edges; judge from the location given. Below 1.2 mm warns, below 0.8 mm fails.
- **interference**: overlap volume between assembled parts. Contact (gap 0.000) is normal for resting parts.
- **slice**: Bambu Studio CLI result, time, grams and which nozzle the filament was mapped to. A slicer failure fails the build.

## FDM design rules (0.4 mm nozzle, 0.2 mm layers)

- Walls >= 1.2 mm (3 lines); structural walls 2+ mm. Floors/roofs >= 1.0 mm (5 layers). Prefer thicknesses that are multiples of ~0.42 mm.
- Parts are weakest across layer lines (Z). Orient so the main bending/tension load runs along layers, not across them. Snap-fit arms and clips should flex in the XY plane.
- Bed-facing edges: 0.4 mm chamfer against elephant foot (`F.elephant_foot_chamfer`). Downward-facing edges elsewhere: chamfer, don't fillet. Fillets are fine on vertical edges and upward-facing edges.
- Horizontal holes: use `F.teardrop` (point up) or accept a slightly drooped top for holes under ~6 mm. Vertical holes print ~0.1-0.2 mm undersize; screw clearances in `F.SCREW_CLEARANCE` already account for that.
- Fasteners: heat-set inserts (`bd_warehouse.fastener.InsertHole` / `HeatSetNut`, CNCKitchen sizes) for anything reassembled; nut traps (`F.hex_nut_trap`) or pilot holes for thread-forming screws (`F.PILOT_HOLE`) otherwise. Printed threads only at M8+ or coarse custom pitches (`bd_warehouse.thread.IsoThread`).
- Clearances (diametral, PLA baseline, see printer.toml): press 0.10, snug 0.20, slide 0.30, loose 0.50. Print-in-place gaps >= 0.3 mm per side.
- Text: emboss/engrave >= 0.6 mm deep, strokes >= 0.5 mm wide, font >= 6 mm tall; on vertical faces or the top face, not on the bed face unless mirrored deliberately.
- Avoid large flat bottoms with sharp corners in ABS-GF (warping): round vertical corners, consider mouse ears.
- Bridges up to ~15 mm are fine; longer ones sag. Break long ceilings with ribs or put them on the bed.

## X2D nozzles

The X2D has two nozzles with different reach. Numbers come from the machine preset (`extruder_printable_area`, `extruder_printable_height`), read at build time; bed coordinates have the origin at the front-left corner.

| Nozzle | Drive | X | Y | Max Z | Volume |
|---|---|---|---|---|---|
| Left (main, extruder 1) | direct drive | 0-256 | 0-256 | 261 | 256 x 256 x 261 |
| Right (auxiliary, extruder 2) | Bowden | 20.5-256 | 0-256 | 256 | 235.5 x 256 x 256 |

- The right nozzle can't reach the leftmost 20.5 mm strip of the bed or the top 5 mm of height. Bambu's spec sheet lists the main nozzle as 260 mm tall; the slicer preset allows 261.
- Multi-material prints, and anything that may be printed on the right nozzle, must stay within the shared 235.5 x 256 x 256 region. Only single-material prints on the left nozzle can use the full 256 x 256 x 261.
- Y is 256 for both nozzles, so a part over 235.5 mm in one direction usually fits both after turning its long side along Y. Do that unless the orientation matters.
- Flexible filament (TPU) goes on the left, direct-drive nozzle; the Bowden right nozzle feeds it poorly.
- `cad build` lays the parts out on the plate itself (shelf packing, 5 mm gaps), inside the shared region when everything fits and the full left-nozzle region otherwise. The slicer's own auto-arrange only uses the shared region and rejects bigger parts, so it is turned off. The CLI maps a single filament to the left nozzle; the report says which nozzle was used. When printing from the GUI with the AMS, the nozzle is chosen at print time.

## Materials on hand

PLA (default), PETG, ABS-GF, TPU 95A (untested on this printer). Per-material slicer presets, extra clearance and notes are in printer.toml `[materials]`. Pick by function: PLA for indoor/prototype, PETG for toughness/heat to ~70 C/outdoors, ABS-GF for stiff hot parts (~95 C) and dimensional stability, TPU for flexible parts (walls >= 1.6 mm, no supports or long bridges).

## build123d notes

- Units are mm. Prefer algebra mode (`part = Box(...) - Pos(...) * Cylinder(...)`); builder mode is fine for sketch-heavy work.
- `Plane.XZ` normal points -Y. When a direction matters, build `Plane(origin=..., z_dir=...)` explicitly.
- Select edges/faces with `.edges().filter_by(Axis.Z)`, `.group_by(Axis.Z)[-1]`, `.sort_by(...)`, or `face.outer_wire().edges()`. Edges of rounded profiles are several line/arc segments.
- Apply fillets and chamfers last; if one fails, reduce the size or select fewer edges.
- bd_warehouse has fasteners, threads, bearings, gears, pipes; prefer it over hand-rolling catalog parts.

## Environment

- Python via uv only (`uv run ...`, `uv add ...`). Python is pinned to 3.13.
- Bambu Studio is the flatpak `com.bambulab.BambuStudio`. The CLI needs flattened presets; `bambu.py` resolves `inherits` chains from the system profiles and the user's presets in `~/.var/app/com.bambulab.BambuStudio/config/BambuStudio/user/`. The default machine preset is the user's `My X2D 0.4 nozzle`.
- The slicer logs a GLFW/OpenGL error when run headless; it only affects plate thumbnails.
- The flatpak only sees `$HOME`, so files passed to the slicer must live under it (not `/tmp`). Tests use `.tmp-test/` (gitignored).

## Git

- One repo for the whole folder, remote on GitHub (`theepicflyer/claude-3d`, private). `out/` is gitignored; it is regenerated by `cad build`.
- Claude owns git here: branch, PR and merge without asking once tests pass and the work has been walked through.
- Work on a branch per project or tooling change, in a worktree under `.worktrees/` (gitignored); run `uv sync` in a new worktree. Open a PR when the build passes and the renders have been reviewed. Merge commits, no force pushes.
- Keep this file, README.md and each project's CLAUDE.md current when behavior or decisions change.
