# Quantix design system

The `--qx-*` tokens in `src/quantixDesignSystem.css` are the canonical design
language for the application. New surfaces and components should consume these
tokens rather than introducing local colors, font stacks, or spacing values.
The [ASA workspace concept](./asa-workspace-concept.png) and
[Settings and Doctor concept](./asa-settings-doctor-concept.png) are the visual
references for the active Tender workspace and control families. They are not
application assets; all visible UI remains code-native.

## Identity

| Token | Value | Use |
| --- | --- | --- |
| `--qx-brand-graphite` | `#3F464D` | Primary identity, text, and dark control surfaces |
| `--qx-brand-silver` | `#9AA3AB` | Secondary logo tone and quiet metadata |
| `--qx-brand-white` | `#FFFFFF` | Inverse text and native contrast states only |
| `--qx-brand-blue` | `#397C9D` | Accessible focus and active-state accent |
| `--qx-brand-slate` | `#6F7782` | Secondary text and evidence metadata |

The product logo is grayscale. Baby blue belongs to structural side planes,
while the stronger blue is reserved for focus and active-state affordances.
Success, warning, and danger keep independent semantic tokens so operational
state remains legible without relying on layout alone.

## Typography

Use `--qx-font-ui` for all product UI, headings, labels, and controls. It is a
Segoe UI Variable-first stack chosen for the Windows desktop product and has
system fallbacks for other platforms. Use `--qx-font-mono` only for diagnostics,
IDs, file paths, and machine-readable values. Do not mix Inter or ad-hoc font
stacks into product surfaces.

Express product type sizes in `rem` so the Engineer's larger-text preference
scales every surface. Visible text must not render below `0.75rem` (12px at the
default root size). Segoe UI Variable weights range from 100 through 700; do not
request values outside that range.

Tag authoritative source text with its normalized BCP 47 language (`ar` or
`en`) and recorded direction. Mixed or undetermined text uses `dir="auto"` and
does not invent a language tag. Keep derived translations direction-aware but
language-neutral until their target language is part of the stored record.

## Surfaces and state

The light product UI uses a near-white baby-blue workspace field, cool gray
controls, and softly deeper baby-blue side planes. `--qx-canvas` and
`--qx-scene-gradient` form one continuous application field;
the root Quantix window owns that field behind the title bar and all workspace
content. Six related `--qx-field-stop-1` through `--qx-field-stop-6` values
provide the light and dark tonal stacks.

Structural surfaces are translucent washes over the field, not opaque cards:
`--qx-wash-titlebar`, `--qx-wash-sidebar`, `--qx-wash-main`,
`--qx-wash-context`, `--qx-wash-section`, `--qx-wash-control`,
`--qx-wash-status`, `--qx-wash-selection`, `--qx-wash-startup`, and
`--qx-wash-popover` are the canonical recipes. Each structural wash reaches
transparency at its element edge so adjacent areas merge through a visible
tonal ramp. The title bar therefore clears before the 35px content seam. Use
these recipes for new
surfaces instead of
adding a solid fill, divider rule, inset outline, backdrop filter, blend mode,
or a gradient that terminates at a boundary. Keep gradients spatially static;
existing motion may animate opacity, selection, and depth envelopes only.

`--qx-surface` is the canonical content plane, `--qx-surface-subtle` is a
quiet inset wash, `--qx-surface-panel` supports
persistent panels, and `--qx-surface-raised` is reserved for transient menus
and raised controls. Use `--qx-hover` for pointer hover and `--qx-selected` for
persistent selection. Border tokens are reserved for form affordances and
focus/native control treatment; structural application planes stay borderless.

The light material values are fixed: canvas and base surface `#F4FAFE`, chrome
`#EDF7FC`, subtle surface `#EEF8FC`, panel `#F1F9FD`, hover `#E6F2F8`,
selection `#DCEEF7`, border `#D8E8F0`, side panel `#EAF6FC`, and side-panel
depth `#DDEFF8`. Dark and system-dark appearances use lighter mist-charcoal
workspace surfaces with muted steel-blue side planes. Operating-system
forced-colors modes
flatten the field to system Canvas and remove custom structural borders and
shadows; system focus rings, spacing, and typography continue to carry the
hierarchy. Quantix does not maintain a separate Higher contrast preference.

Interactive controls use `--qx-action` and `--qx-action-hover`. Every keyboard
reachable control must retain the shared `:focus-visible` treatment using
`--qx-focus`; do not remove the outline for a custom shadow. Select, Listbox,
Menu, Popover, Switch, Dialog, and Tooltip are Quantix-owned React Aria
components. Native browser popups are not part of the active design system.
Their typography, surface, selection, disabled, focus, and forced-colors states
must be defined by Quantix tokens.

## Layout and components

Use the 4px spacing scale (`--qx-space-1` through `--qx-space-12`) and the
shared control/panel radii. Component families are:

- navigation: app rail, workspace switcher, tender list, settings navigation;
- actions: primary, secondary, quiet, destructive, and icon buttons;
- surfaces: panels, cards, popovers, dialogs, and empty states;
- evidence: status chips, metadata rows, progress, and diagnostics;
- intake: package registration, local-processing, provider approval, and review.

The Manager composer is one raised surface with a multiline message field and
a separate footer. Its left side owns `Tools & Context`; its right side uses a
concise AI summary trigger that opens the exact Provider, Model, and Reasoning
selectors alongside Send. It never shows a generic full-access mode.
Tender-row management uses one row-level Menu
from ellipsis, keyboard menu key, or secondary click. Quantix Doctor findings
use an open list with cause, impact, and one typed action rather than a card
grid or generic command runner.

Prefer one clear primary action per surface. Keep evidence and status readable
without relying on color alone. Structural application planes do not use
elevation; reserve `--qx-shadow-raised` and `--qx-shadow-popover` for
interactive composers, transient menus, drawers, and dialogs. All shadows
disappear in operating-system forced-colors mode.

## Motion and accessibility

Use the named motion durations and `--qx-motion-ease` for interface transitions.
Routine controls use only short functional transitions. The approved cinematic
exception is limited to one settled workspace arrival and shallow,
pointer-driven ambient parallax behind the interactive shell. It never animates
status, evidence, Agents, progress meaning, reading content, the title bar, or
native window controls, and it must not replay during Tender or view navigation.

Both the operating-system `prefers-reduced-motion` setting and Quantix's saved
Reduce motion preference are authoritative. Under either one, render the final
workspace state immediately, remove ambient transforms, and do not attach
pointer-driven motion. Essential state changes must remain understandable
without animation. Coarse/no-hover pointers receive the same static ambient
composition. Maintain contrast in light, dark, system, and forced-colors modes,
and preserve `:focus-visible` throughout.
