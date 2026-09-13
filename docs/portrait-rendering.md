# Staff portrait rendering

Quantix staff portraits are local decorative UI assets. The server stores a
stable descriptor with each generated staff identity and version:

```json
{ "style": "notionists-v1", "seed": "<server-owned staff id>" }
```

The UI currently recognises only `notionists-v1`. It imports the single local
`@dicebear/styles/notionists.json` definition and renders it with the DiceBear
core `Avatar` and `Style` APIs. The internal SVG render size is fixed at 160 ×
160; `StaffPortrait` may scale that image for a card or a compact staff list.
The recipe also sets `idRandomization: false` so SVG IDs remain deterministic.
The seed is the server descriptor, never the display name, role or title, so a
rename, profile revision or theme change does not change the portrait.

## Pinned recipe

The recipe is pinned to `@dicebear/core` 10.7.0 and `@dicebear/styles` 10.6.0.
The style ID remains `notionists-v1` only while this recipe remains unchanged.
If the dependency or local definition changes, update the style ID and its
golden hash together instead of silently changing an existing identity.

For the seed `quantix-manager-24`, the SHA-256 of the raw SVG produced by the
recipe is:

```text
8a5311ee65907e892beee75b831f51abe5928812fdc7c8a30c88170b4fb2fc55
```

The focused `StaffPortrait.test.tsx` test renders the installed library and
checks this hash. It also checks that changing only the display name keeps the
same data URI and that different synthetic seeds produce different output.

## Failure and accessibility behavior

The renderer accepts bounded server style and seed values and never treats a
profile field as a URL or SVG document. A missing/unknown style, invalid seed,
or synchronous local render failure shows Unicode-aware initials with a short
reason. An image load failure does the same and offers a local retry. Staff
work remains available in every fallback state.

Normal portraits have meaningful image alt text. Decorative portraits use an
empty alt and are hidden from assistive technology; fallback status remains
available in the surrounding UI when it carries an actionable failure.

The Notionists style is by Zoish and is licensed CC0 1.0. DiceBear's core and
style package code is MIT licensed. Source and license details are retained in
the installed package metadata and the official references:

- [DiceBear JavaScript API](https://www.dicebear.com/integrations/javascript/)
- [DiceBear Notionists style](https://www.dicebear.com/styles/notionists/)
- [DiceBear styles license](https://github.com/dicebear/styles/blob/main/LICENSE.md)
- [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/)

Portraits are presentation data only. They are not Tender evidence and are not
stored under imported originals.
