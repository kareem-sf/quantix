# Quantix brand

**The mark: Weave.** A pure X where one stroke passes under the other, the way the QS ribbon passes itself.
The ribbon is copper; the upper half of the under-stroke is the one accent piece, ivory on dark and ink on light.
[GUIDELINES.md](GUIDELINES.md) has the rules: clear space, minimum sizes, lockups and what not to do.

Quantix is part of the **QS Mind** house (estimate → tender → procure), and Quantix is its tender stage. Outside
the product, such as on a website, in a deck or a store listing, use the endorsed lockup with its **by QS Mind** line.
Inside the product the plain mark or lockup is enough.

## Colours

| Use | Colour |
| --- | --- |
| Ribbon, on dark and on light | `#E0703A` |
| Accent piece on dark | Ivory `#F3EFE6` |
| Accent piece on light | Ink `#1F2328` |
| Stage (dark background) | `#0E1013` |

The same values are CSS custom properties in [tokens.css](tokens.css).

## Files

| Folder | What's in it |
| --- | --- |
| `logo/` | SVG masters, pure paths: the mark (`quantix-mark-solid-on-dark`, `-on-light`, `-white`, `-black`), the horizontal and stacked lockups, and the endorsed lockup (`quantix-lockup-endorsed-on-dark`, `-on-light`) |
| `icons/` | App icon (`app-icon.svg`, `app-icon-1024.png`, `icon.ico`, `icon.icns`), favicons, touch and PWA icons, `site.webmanifest`. No tile: the mark in its on-light colours on a transparent square. Only the maskable icon keeps its tile, since Android cuts it to a shape |
| `png/` | Every logo master rendered as a PNG (lockups 2000 px wide, marks 1024 px) for documents, decks and anywhere SVG can't go |
| `social/` | `og-image-1200x630.png` for link previews |
| `founder/` | Kareem Safwat's K mark and signature, on dark and on light |
| `fonts/` | Urbanist, the brand typeface (variable weight), with its SIL Open Font License |
| `tokens/` | The QS Mind house's colours and type as CSS custom properties, JSON and a Tailwind theme; `tokens.css` here is Quantix's own |
| `brand-book.html` | The QS Mind brand book: the house, each product's mark and how they are used together. Open it in a browser |

The desktop icons in `desktop/icons/` are generated from `icons/app-icon-1024.png`, and the interface's favicon is
`icons/favicon.svg`. The wordmark is set in Urbanist Light and outlined in every master, so no font is needed to show
the logo; `fonts/` is for setting new brand text, such as a deck or a web page.

## Founder

Quantix is founded and developed by [Kareem Safwat](https://kareemsafwat.com). His signature, `founder/signature-*`,
credits him on About screens, website footers and under each repository's README header. It never goes inside the logo.
