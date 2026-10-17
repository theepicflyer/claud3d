# claude-3d

A code-driven CAD setup for designing 3D-printed parts with Claude Code. You describe what you need; Claude writes a parametric [build123d](https://github.com/gumyr/build123d) model, then builds, checks, renders and slices it for a Bambu Lab X2D.

## Setup

Requirements: [uv](https://docs.astral.sh/uv/), and Bambu Studio (flatpak `com.bambulab.BambuStudio`) for slicing. FreeCAD is optional, for viewing STEP files.

```
uv sync
uv run pytest -q
uv run cad build example-box
```

## Usage

```
uv run cad new my-part          # creates projects/my-part/{CLAUDE.md, model.py}
uv run cad build my-part        # writes projects/my-part/out/
uv run cad build my-part -p wall=2.4
uv run cad open my-part         # opens the sliced project in Bambu Studio
uv run cad list
```

A build writes these files to `out/`:

- `<part>.step` and `<part>.stl` for each part
- `<part>.png`: iso, below, front, right and top views, plus a cross-section. Overhangs are red, flat ceilings orange and thin walls magenta.
- `assembly.png`, when the model defines an assembly
- `report.md` / `report.json`: printability checks (manifold, fits the build volume, bed contact, overhangs, bridges, wall thickness, interference) and the project's own requirement checks
- `slice/<project>.gcode.3mf`: sliced with the X2D, process and filament presets, ready to open or send to the printer

Printer, presets, design limits, fit clearances and materials are in `printer.toml`. Each project's requirements, decisions and print log live in its own `CLAUDE.md`. The root `CLAUDE.md` describes the workflow Claude follows.
