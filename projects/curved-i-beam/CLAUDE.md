# curved-i-beam

A small I-beam bent into a 25° wedge: the two flanges are flat end plates 25° apart, and the web between them has arc-shaped inner and outer edges, on radii of 24.4 and 67.4 mm, so the I is 10.6 mm tall at the inner edge and 29.4 mm at the outer. It takes a compressive load between the flanges.

## Requirements

Numbered so `check()` in model.py can reference them. Each has a measurable acceptance criterion.

- R1: the inner arc has a radius of 24.38 mm, ±0.05 mm (10.6 mm tall along the arc between the flanges' outer faces).
- R2: the flanges are 43 mm long (inner arc to outer arc), ±0.05 mm.
- R3: the flanges' outer faces lie on radii 25° apart, ±0.05°.
- R4: the I is 10 mm wide (flange width), ±0.01 mm.
- R5: web and flanges are 1 mm thick, ±0.01 mm.
- R6: 2 mm fillets join the web to both flanges, on both sides (4 fillets).
- R7: prints without support: no flat downward face above the bed is steeper than 42° from vertical.

## Interfaces and measurements

None given. Derived: outer radius 24.4 + 43 = 67.4 mm, so the outer edge is 29.4 mm tall along the arc.

## Print

- Material: PLA
- Orientation: standing on one flange, web vertical, the other flange leaning at 25° on top.
- Supports: none. The top flange's underside slopes down toward the web so it stays within 42° of vertical.
- Slicer changes from defaults: none

## Decisions

- The flanges are the end plates and the web carries the arcs (first draft had it the other way round, with curved flanges; the user corrected it). Heights are arc lengths between the flanges' outer faces; the radii and 21.5 mm flange length follow.
- Printed standing on a flange. Lying flat would put the web at mid-height as a 20-35 mm bridge with free edges. Standing up, the top flange's wings would overhang; its underside is tapered instead (see below). Layer lines run across the web; the load is compressive, so that is fine for use.
- The flange ends are trimmed to the same arcs as the web's edges, so the inner and outer faces each run as one continuous surface from flange to flange (review #1: a flat flange end met the curved web edge with a 0.01 mm step). The trim band's rounded cross-section rounds the flange corners 1 mm, so the bed footprint has no sharp corners.
- Second revision (after the first print): flanges doubled to 43 mm and the angle reduced to 30°, both at the user's request. The inner height is kept at 20 mm, so the inner radius grows to 38.2 mm and the outer height follows (42.5 mm). The top flange's underside is now 60° from vertical, steeper than before; still unsupported.
- Third revision: angle reduced to 25° and the inner height to 15 mm, at the user's request; flanges stay 43 mm. The top flange's underside is now 65° from vertical, still unsupported.
- Fourth revision: the beam moved 10 mm toward the arc centre, at the user's request, keeping the 25° angle and 43 mm flanges. Inner radius 34.4 → 24.4 mm and outer 77.4 → 67.4 mm, so the inner height drops from 15 to 10.6 mm and the outer from 33.8 to 29.4 mm. R1 is now the inner radius (`inner_r`) instead of the inner arc length, since that is what the user specified. Near the inner edge the top flange's taper (5.1 mm at the web) and the two 2 mm root fillets now take up most of the 10.6 mm, leaving under 1 mm of straight web there. R5 measures the web at mid-radius, because the mid-sweep section now cuts through the top flange's taper near the inner edge.
- The top flange's underside no longer runs parallel to its top face (user's suggestion). It is tilted 42° about the flange's long axis, sloping down toward the web, so the flange is 1 mm thick at its edges and 5.1 mm at the web. That keeps the underside at 42° from vertical at the 25° sweep (3° margin under the 45° rule); `taper()` recomputes it from `sweep` and `underside_max`. The bottom flange stays a flat 1 mm plate since it sits on the bed.
- 2 mm root fillets where the web meets the flanges, because the 1 mm web separated along a layer when the part was taken off the bed. The fillets spread the joint over more area per layer.
- Web and flanges are 1 mm (user's choice after seeing the 3 mm version), below the 1.2 mm wall guideline, so the wall thickness WARN is expected. That is about two extrusion lines.

## Print log

- 2026-10-21, 40° / 21.5 mm flanges / 1 mm walls, PLA, default 0.20 mm process, no supports. Result "not bad", but the layers separated at the web-to-flange joint when it was pulled off the bed. Changes: 2 mm root fillets between web and flanges, flanges doubled to 43 mm, angle reduced to 30°.
