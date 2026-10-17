# cutting-board-stand

A drying stand for the kitchen counter nook right of the sink. It holds two wooden cutting boards and, sometimes, a baking sheet on edge, leaning back toward the wall, with the faces open to the air so they dry.

## Requirements

Counter frame used below: X runs along the back wall from the sink's right edge (x=0) to the side wall (x=300). Y runs out from where the back wall's curve meets the counter (y=0) toward the room. Z is up from the counter.

- R1: Holds the big board, the small board and the baking sheet at the same time, each in its own slot. The sheet stands on its 280 mm edge.
- R2: Each item lifts straight up out of its slot without moving the stand or the other items. Measured as: no stand geometry above an item's slot within its lift path.
- R3: The stand, the small board and the baking sheet project inside x 0–300, y 0–210. The big board does too, except for the part left of x=0, which stays within y 0–80 (over the strip behind the sink).
- R4: Nothing touches the wall. Every part of the stand and every item stays at y ≥ 0.
- R5: Boards dry on both faces. The stand touches each item only along its bottom edge and on narrow ridges against its back face. Neighbouring items are at least 25 mm apart face to face. Stand geometry within 5 mm of a face covers at most 5% of that face.
- R6: Stable. For every combination of loaded and empty slots, the combined centre of mass sits inside the feet's footprint with at least 20 mm margin.
- R7: Drains. The frame is open and touches the counter only through feet pads, at most 10% of its plan area. The arches between feet are at least 5 mm high at their crown. Every up-facing surface is a sloped or ridged strip; no level patch is more than 2 mm across, so water can't pool.
- R8: Prints in PETG on the X2D without supports.

## Interfaces and measurements

Measured by the user with a tape measure (2026-10-17).

- Counter nook: 300 mm wide (from the sink's right edge to the side wall) x 210 mm deep (from the back wall). Both measured from where the wall curve meets the counter.
- Back wall base: a curved upstand with about a 13 mm radius. The tile face sits about 13 mm behind y=0.
- Strip behind the sink: 80 mm deep (left of x=0).
- Big cutting board: 355 x 355 x 20 mm, wood, rounded corners.
- Small cutting board: 235 x 235 x 20 mm.
- Baking sheet: 370 x 280 x 25 mm overall including the rim, no handles. Used occasionally.

## Print

- Material: PETG (wet location, hot water). The user may add a denser-infill modifier to the lower part for weight.
- Orientation: upright, as it stands on the counter; the feet are on the bed.
- Supports: none. Arches have 45° flanks and 12 mm flat bridges; prongs lean 8°.
- Slicer changes from defaults: plate type set to Textured PEI (PETG isn't allowed on the default Cool Plate).
- First slice: 4h 25m, 86 g.

## Decisions

- Every item is longer than the 210 mm depth, so all three stand with their faces parallel to the back wall, one behind another.
- Order from the back: big board, baking sheet, small board. The big board has to be at the back so its 55 mm overhang past x=0 lands on the 80 mm strip. The sheet slot sits in the middle, so when it's empty the two boards get extra air between them.
- The stand holds everything up on its own. Nothing leans on the wall tiles (user, 2026-10-17).
- The boards lean back 8° rather than 10°, which keeps the big board within the 80 mm strip.
- Open frame on feet instead of a drip tray (user, 2026-10-17).
- One piece printed upright, not a kit of flat-printed combs joined with screws. Joints would be crevices that stay wet, and the loads are small: the big board pushes about 1 N on each prong. Prongs taper from 14 mm deep at the root to 8 mm at the tip, which roughly doubles root strength against knocks.
- Two side combs 185 mm apart (centres), inside the small board's width with 25 mm to spare each side, joined by three cross rails (back, middle, front).
- Every exposed upper face (ledges, lips, prong fronts and tops, beam and rail tops) is chamfered at 45° from both sides down to a 1 mm ridge. Items touch only these ridges, and water runs off them sideways.
- Lip is 6 mm tall with 1.5 mm clearance to the item's front face. A vertical lift moves the item's front face forward by tan(8°) per mm of lift, so the clearance has to exceed 6 x tan 8° = 0.84 mm.
- R6 mass assumptions: wood 0.75 g/cm³ (big board about 1.9 kg), baking sheet 1 kg, stand at 30% effective fill. Heavier infill in the base only improves it.

## Print log

Not printed yet.
