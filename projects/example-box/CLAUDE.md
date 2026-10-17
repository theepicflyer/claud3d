# example-box

Reference project for the environment: a small storage box with a friction-fit lid. Use it as a pattern for multi-part projects with an assembly and spec checks.

## Requirements

- R1: Usable cavity of at least 60 x 40 x 30 mm (`inner_x`, `inner_y`, `inner_z`).
- R2: Walls and floor 2 mm thick.
- R3: Lid lip fits the opening with the `snug` clearance from printer.toml (0.20 mm total for PLA, 0.10 mm per side), so the lid holds by friction but comes off by hand. Lip is 3 mm tall.
- R4: A fingertip can get under the lid to open it: a 20 mm wide, 5 mm deep scallop in the rim of the +X wall.

## Interfaces and measurements

None. Self-contained.

## Print

- Material: PLA (Bambu PLA Basic)
- Orientation: box opening up; lid printed upside down (top face on the bed, lip pointing up). No supports needed.
- Slicer changes from defaults: none

## Decisions

- Lip has a 0.4 mm lead-in chamfer at its tip. A 0.6 mm chamfer left the tip 1.0 mm thick and tripped the thin-wall warning.
- Bed edges get a 0.4 mm chamfer against elephant foot, so the lid's outline and the box's footprint stay true.
- Review #1 (2026-10-17): the 5 mm lip was too tall; cut to 3 mm, which still guides and holds the lid.
- Review #2 (2026-10-17): the lid sat flush with nothing to grip. Added a circular scallop in the box's +X rim (not the lid) so a fingertip can lift the lid's edge. It's one end only, and the lid fits either way round because the scallop is in the box. Scallop edges have 0.8 mm fillets. They face up or sideways, so they print without overhangs.

## Print log

Not printed yet.
