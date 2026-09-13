# Quantix identity studies — version 2

Three directions developed after researching the official OpenAI and xAI/Grok materials and Geist's Anthropic case study. The research and source links are in `research.md`.

## Directions

- **01 / Continuum:** a continuous Q with a clear circular counter and an integrated diagonal foot, paired with a bold lowercase wordmark. Recommended starting direction.
- **02 / Junction:** four broad, symmetrical diagonal forms around an open central diamond, paired with the same lowercase wordmark.
- **03 / Editorial:** a restrained Georgia wordmark, with a Q derived from that typography for icon use.

These are alternative identity directions. No direction has been recorded as the user's final selection.

## Files

The complete asset folder is `~/.quantix/outputs/brand/quantix-v2-2026-09-09/`.

- `quantix-v2-directions-4k.png`: 3840 × 2400 comparison board.
- `quantix-{direction}-lockup-{black,white}.svg`: scalable outlined wordmarks and symbols, with no external font dependency.
- `quantix-{direction}-mark-{black,white}.svg`: scalable standalone symbols.
- `quantix-{direction}-lockup-4k.png`: transparent 3840 × 1440 artwork rendered in Blender.
- `quantix-{direction}-mark-1k.png`: transparent 1024 × 1024 symbols rendered in Blender.
- `quantix-{direction}-icon-{32,64}.png`: small icons rasterized from the SVG masters.
- `quantix-continuum-sculptural.png`: 2048 × 1280 matte 3D presentation of Continuum.
- `Quantix-Identity-v2.blend`: editable comparison, identity and sculptural scenes. The final comparison scene is named `Quantix | Identity v2`.
- `quantix_brand_v2.py`: reproducible Blender scene and SVG builder.

The ink is `#151817`, the light background is `#f6f5f0`, and the inverse artwork is `#fbfaf6`. SVG files use these sRGB values; the flat Blender materials convert them to linear color before rendering. Physical lighting changes the appearance of the separate sculptural presentation.

## Editing and use

Use SVG masters for scaling and layout. Preserve their proportions and the open spaces inside each mark. The built-in SVG canvas provides margin; add room appropriate to the surrounding layout. Use the flat artwork for the primary identity, and use the sculptural treatment as a presentation asset.

Blender text remains editable, with source fonts packed into the native file. The SVG letters are closed outlines. Identity scenes have separate lockup and icon collections and cameras. Sculptural scenes reuse the same underlying geometry with matte materials and studio lighting.

Artifact checks covered the rendered comparison, SVG outlines on both backgrounds, exported dimensions and alpha, and native-file readability. These checks concern logo assets only.
