# cutting-board-stand

A drying stand for the kitchen counter nook right of the sink. It holds two wooden cutting boards and, sometimes, a baking sheet on edge, leaning back toward the wall, with the faces open to the air so they dry.

## Requirements

Counter frame used below: X runs along the back wall from the sink's right edge (x=0) to the side wall (x=300). Y runs out from where the back wall's curve meets the counter (y=0) toward the room. Z is up from the counter.

- R1: Holds the big board, the small board and the baking sheet at the same time, each in its own slot. The sheet stands on its 280 mm edge. The small board also fits the sheet's slot (user, 2026-10-17).
- R2: Each item lifts straight up out of its slot without moving the stand or the other items. Measured as: no stand geometry above an item's slot within its lift path.
- R3: The stand, the small board and the baking sheet project inside x 0–300, y 0–210. The big board does too, except for the part left of x=0, which stays within y 0–80 (over the strip behind the sink).
- R4: Nothing touches the wall. Every part of the stand and every item stays at y ≥ 0.
- R5: Boards dry on both faces. The stand touches each item only along its bottom edge and on the rounded top of the finger behind it. Neighbouring items are at least 25 mm apart face to face. Stand geometry within 5 mm of a face covers at most 5% of that face.
- R6: Stable. For every combination of loaded and empty slots, the combined centre of mass sits inside the feet's footprint with at least 20 mm margin.
- R7: Drains. The frame is open and stands on the user's stick-on silicone bumpers, so it sits off the counter; its underside is flat so they sit flush. No level up-facing patch is more than 3 mm across, so water can't pool.
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
- Orientation: upright, as it stands on the counter; the flat underside is on the bed.
- Supports: none. Nothing overhangs; the fingers lean at most 5°.
- Slicer changes from defaults: plate type set to Textured PEI (PETG isn't allowed on the default Cool Plate).
- After printing: stick silicone bumpers under both ends of each panel (four in all).
- Slice: 4h 07m, 132 g.

## Decisions

- Every item is longer than the 210 mm depth, so all three stand with their faces parallel to the back wall, one behind another.
- Order from the back: big board, baking sheet, small board. The big board has to be at the back so its 55 mm overhang past x=0 lands on the 80 mm strip. The sheet slot sits in the middle, so when it's empty the two boards get extra air between them.
- The stand holds everything up on its own. Nothing leans on the wall tiles (user, 2026-10-17).
- The boards lean back 8° rather than 10°, which keeps the big board within the 80 mm strip.
- Open frame on feet instead of a drip tray (user, 2026-10-17).
- One piece printed upright, not a kit of flat-printed parts joined with screws. Joints would be crevices that stay wet, and the loads are small: the big board pushes about 1 N on each panel.
- Two side panels 185 mm apart (centres), inside the small board's width with 25 mm to spare each side, joined by three round-topped rails, each under a finger rather than a slot.
- Review #1 (2026-10-17): the first version (thin prongs, stepped ledges and lips, 45° ridge chamfers, arches under the base) looked medieval and faceted, and separately chamfered pieces met in mismatched junctions. The user asked for something curvier with a modern home look. Replaced with two side panels: a flat base strip with four leaning fingers whose heights follow one sweep from the tall back to the low front. Each finger has a round cap (both corners about half its top width), every inside corner is rounded, and every exposed edge has a 4.5 mm round. The profile is one 2D outline, filleted and then edge-rounded as a whole, so there are no seams between pieces.
- Each item rests on the round cap of the finger behind it. The finger's front edge stands 3° more upright than the item, so the gap opens downward and only the cap touches. The model solves the slot position so the cap is tangent to the item's back face.
- Slot front walls are vertical with 5 mm clearance at the floor. Lifting a leaning item straight up moves its front face forward by tan 8° per mm, so a front wall leaning with the item would catch it.
- Review #2 (2026-10-17): the finger behind the sheet was 12 mm wide at the top, between the 33 mm back finger and the 17 mm one behind the small board, which read as thick-thin-thick. The user also wants to be able to put the small board in either middle slot. The front wall of the big board's slot now leans back about 3°, just enough to bring that finger to 18 mm, matching the small board's (`rest_min`). It stays upright enough for a straight lift: the board's front-bottom corner would only reach the wall about 99 mm up, above the top of the finger.
- Feet are the user's stick-on silicone bumpers (2026-10-17). The underside is flat, and there are no printed feet or arches.
- R6 mass assumptions: wood 0.75 g/cm³ (big board about 1.9 kg), baking sheet 1 kg, stand at 30% effective fill. Heavier infill in the base only improves it.

## Print log

Not printed yet.
