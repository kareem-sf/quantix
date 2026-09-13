# Quantix brand assets

The active app identity is the complete supplied [transparent v4 package](v4/README.md). See [the v4 integration mapping](v4-integration.md) for app components, theme colors, web assets and desktop icons. The package is retained unchanged.

## Historical 3D logo

An architectural Q formed from three coordinated structural segments, with a teal diagonal stroke and a narrow teal inlay. Titanium faces, graphite sidewalls, softened machined edges, and a navy studio backdrop give the mark a construction-engineering character.

The palette follows the application's navy `#111d2b` and teal `#117d76`. Physical materials and studio reflections produce lighter and darker shades in the render. The wordmark uses Bahnschrift with editable Blender text.

## Deliverables

The generated files are under `~/.quantix/outputs/brand/quantix-3d-2026-09-09/`:

- `Quantix-3D-Logo.blend` — native scene, geometry, bevel modifiers, materials, packed font, editable lettering, studio lights, and two cameras. Open the `Quantix | Final Logo` scene.
- `quantix-hero-4k.png` — 3840 × 2160 studio presentation.
- `quantix-logo-transparent-4k.png` — 3840 × 2160 transparent logo and wordmark.
- `quantix-emblem-transparent-2k.png` — 2048 × 2048 transparent emblem.

The final renders use Blender Cycles with 64 samples, adaptive sampling, and denoising. The studio image includes physical shadows; transparent exports hide the studio backplate.

## Editing

The scene separates the emblem, wordmark, and studio into named collections. Rotate or scale the Quantix root to move the complete design. Adjust individual bevel modifiers for edge softness, and edit either text object to change the lettering. The Icon Camera frames the emblem; hide the Wordmark collection and the Navy Backplate for a transparent icon.

`quantix_logo_scene.py` recreates the scene in Blender without deleting existing scenes. Its builder returns scene and object names and starts with preview settings. The delivered `.blend` contains the final render settings.
