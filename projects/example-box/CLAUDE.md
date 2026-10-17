# example-box

Reference project for the environment: a small storage box with a friction-fit lid. Use it as a pattern for multi-part projects with an assembly and spec checks.

## Requirements

- R1: Usable cavity of at least 60 x 40 x 30 mm (`inner_x`, `inner_y`, `inner_z`).
- R2: Walls and floor 2 mm thick.
- R3: Lid lip fits the opening with the `snug` clearance from printer.toml (0.20 mm total for PLA, 0.10 mm per side), so the lid holds by friction but comes off by hand. Lip is 3 mm tall.
- R4: A fingertip can get under the lid to open it: a 14 mm wide, 2.2 mm deep scallop in the rim of the +X wall. Through the scallop you see only the lid's lip, never the inside of the box.

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
- Review #3 (2026-10-17): the 20 x 5 mm scallop looked heavy and showed the inside of the box below the 3 mm lip. Shrunk it to 14 x 2.2 mm (a shallow arc with a 14 mm chord), so its bottom stays 0.4 mm above where the lip's face ends at the lead-in chamfer. A check now measures that margin on the assembly.

## Print log

Not printed yet.
