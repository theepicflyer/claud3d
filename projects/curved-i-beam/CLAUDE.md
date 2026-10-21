# curved-i-beam

A small I-beam bent into a 25° wedge: the two flanges are flat end plates 25° apart, and the web between them has arc-shaped inner and outer edges, so the I is 15 mm tall at the inner edge and 33.8 mm at the outer. It takes a compressive load between the flanges.

## Requirements

Numbered so `check()` in model.py can reference them. Each has a measurable acceptance criterion.

- R1: the I is 15 mm tall at the inner edge, measured along the arc between the flanges' outer faces, ±0.1 mm.
- R2: the flanges are 43 mm long (inner arc to outer arc), ±0.05 mm.
- R3: the flanges' outer faces lie on radii 25° apart, ±0.05°.
- R4: the I is 10 mm wide (flange width), ±0.01 mm.
- R5: web and flanges are 1 mm thick, ±0.01 mm.
- R6: 2 mm fillets join the web to both flanges, on both sides (4 fillets).

## Interfaces and measurements

None given. Derived: inner radius 15 / 0.436 = 34.4 mm, outer radius 34.4 + 43 = 77.4 mm, so the outer edge is 33.8 mm tall along the arc.

## Print

- Material: PLA
- Orientation: standing on one flange, web vertical, the other flange leaning at 25° on top.
- Supports: none. The top flange's underside overhangs the web by 4.5 mm on each side at 65° from vertical.
- Slicer changes from defaults: none

## Decisions

- The flanges are the end plates and the web carries the arcs (first draft had it the other way round, with curved flanges; the user corrected it). Heights are arc lengths between the flanges' outer faces; the radii and 21.5 mm flange length follow.
- Printed standing on a flange. Lying flat would put the web at mid-height as a 20-35 mm bridge with free edges. Standing up, the only issue is the top flange's 4.5 mm wings (overhangs WARN), accepted without supports. Layer lines run across the web; the load is compressive, so that is fine for use.
- The flange ends are trimmed to the same arcs as the web's edges, so the inner and outer faces each run as one continuous surface from flange to flange (review #1: a flat flange end met the curved web edge with a 0.01 mm step). The trim band's rounded cross-section rounds the flange corners 1 mm, so the bed footprint has no sharp corners.
- Second revision (after the first print): flanges doubled to 43 mm and the angle reduced to 30°, both at the user's request. The inner height is kept at 20 mm, so the inner radius grows to 38.2 mm and the outer height follows (42.5 mm). The top flange's underside is now 60° from vertical, steeper than before; still unsupported.
- Third revision: angle reduced to 25° and the inner height to 15 mm, at the user's request; flanges stay 43 mm. The top flange's underside is now 65° from vertical, still unsupported.
- 2 mm root fillets where the web meets the flanges, because the 1 mm web separated along a layer when the part was taken off the bed. The fillets spread the joint over more area per layer.
- Web and flanges are 1 mm (user's choice after seeing the 3 mm version), below the 1.2 mm wall guideline, so the wall thickness WARN is expected. That is about two extrusion lines.

## Print log

- 2026-10-21, 40° / 21.5 mm flanges / 1 mm walls, PLA, default 0.20 mm process, no supports. Result "not bad", but the layers separated at the web-to-flange joint when it was pulled off the bed. Changes: 2 mm root fillets between web and flanges, flanges doubled to 43 mm, angle reduced to 30°.
