# example-box

Reference project for the environment: a small storage box with a friction-fit lid. Use it as a pattern for multi-part projects with an assembly and spec checks.

## Requirements

- R1: Usable cavity of at least 60 x 40 x 30 mm (`inner_x`, `inner_y`, `inner_z`).
- R2: Walls and floor 2 mm thick.
- R3: Lid lip fits the opening with the `snug` clearance from printer.toml (0.20 mm total for PLA, 0.10 mm per side), so the lid holds by friction but comes off by hand.

## Interfaces and measurements

None. Self-contained.

## Print

- Material: PLA (Bambu PLA Basic)
- Orientation: box opening up; lid printed upside down (top face on the bed, lip pointing up). No supports needed.
- Slicer changes from defaults: none

## Decisions

- Lip has a 0.4 mm lead-in chamfer at its tip. A 0.6 mm chamfer left the tip 1.0 mm thick and tripped the thin-wall warning.
- Bed edges get a 0.4 mm chamfer against elephant foot, so the lid's outline and the box's footprint stay true.

## Print log

Not printed yet.
