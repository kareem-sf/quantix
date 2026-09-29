# Quantix — brand guidelines

**Role in the house:** Stage 2 · Tender — Tender.
**Descriptor:** Your tendering office, on your desktop.

## The mark
Weave: a pure X. One stroke passes under the other, exactly as the QS ribbon passes itself.

Drawn in the QS ribbon grammar: one constant ribbon width, 163-unit outer corners, 45° diagonals and cuts, a 26-unit gap wherever one piece passes another, and at most one separate accent piece. Masters are pure lines and arcs (no clips, no filters), so they import cleanly into Figma, Illustrator and code.

## Colour
| Use | Colour |
|---|---|
| Ribbon (on dark) | `#E0703A` |
| Accent piece (on dark) | Ivory `#F3EFE6` |
| On light backgrounds | `#E0703A` + ink `#1F2328` |
| Stage / background | `#0E1013` |

`#E0703A` has 5.94:1 contrast on the stage. `#E0703A` is the same hue, deepened to
3.21:1 on white (WCAG 3:1 for graphics), so the mark never goes pale on light pages.

## Typography
Urbanist (SIL OFL, `brand/fonts/`). Wordmark: Light 300, outlined in every master. Headlines: ExtraLight 200
or Light 300. Body and UI: Regular 400. Labels: Medium 500 with wide tracking.

## Lockups
- **Horizontal** (default): the solid mark with the wordmark, cap height 0.44 × mark height, gap 0.36 × mark height.
- **Stacked**: for square spaces, splash screens and social avatars.
- **Endorsed**: with the by QS Mind line (see below).


## Clear space and minimum size
Keep one ribbon width clear on every side (it is already built into every file's padding).
Minimum size: mark 20 px tall on screen; lockup 28 px tall.

## Endorsement
Where Quantix appears outside its own product (a website, a deck, a store listing), use
`logo/quantix-lockup-endorsed-*.svg`: the lockup with a small **by QS Mind** line. Inside the product,
the plain lockup is enough.

## Founder signature
Every product credits its founder with `brand/founder/signature-*.svg`: Kareem Safwat's K, unchanged,
with **Founded & developed by Kareem Safwat**. Use it on About screens, website footers, the end card
of the logo animation, and under each repository's README header. It never goes inside the logo.

## Don't
- Recolour the ribbon outside this palette, or give the accent piece the ribbon colour.
- Add gradients, shadows or outlines (the only line version is QS Mind's official line mark).
- Stretch, rotate, or re-space the lockup; always use the supplied files.
- Set the wordmark in live text in another font.
