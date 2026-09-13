# Quantix desktop Tender Office design

Selected primary reference: [manager-concept.png](./manager-concept.png).

This is the selected design direction under the user's authorization to choose the design. It is a fictional populated state for visual definition, not seeded app data and not evidence of completed document analysis. The delivered app opens against real local state.

## Scope and design decision

A local Windows app for one engineer. One Tender Manager is the primary counterpart and coordinates analysis and specialist work. The interface makes findings, sources, planned work, and the engineer's next action visible together.

The main surface is open white paper, anchored by a dark navy tender sidebar. A quiet right rail holds context; it does not compete with the central manager conversation. Teal is reserved for navigation, important actions, and verified state. No decorative illustrations, marketing sections, model mascots, graphs, or invented statistics.

The generated image is a complete desktop screen. It is a reference only: all visible text, controls, iconography, lists, tabs, and borders must be implemented as native components. Do not render the screenshot as the app, crop it into controls, or ship it as a runtime asset.

## Reference dimensions and composition

Generated native dimensions: **1505 × 1045**. Requested design viewport: **1440 × 1000**. Aspect ratios are effectively identical. Preserve the generated composition at the target viewport using these rounded values, and use the native dimensions for exact screenshot comparison when possible.

| Region | At 1440 × 1000 |
| --- | --- |
| Sidebar | x 0, width 274, full height |
| Main header | x 310, y 26; right margin 26 |
| Main title | x 310, y 25 |
| Header tabs | y 123–166, spanning main and context rail |
| Content region | x 310, width 744; right gutter 38 |
| Context rail dividing rule | x 1092, starts at tab baseline |
| Context content | x 1125, width 290, padding top 35 |
| Manager heading | y 199 |
| Analysis heading | y 303 |
| Plan heading | y 580 |
| Plan table | y 642–770 |
| Plan controls | y 779 |
| Composer helper | y 836 |
| Composer | x 307, y 867, width 752, height 103 |
| Sidebar settings divider | y 860 |

At desktop widths, the sidebar has fixed width 274px and the context rail 348px; the conversation fills remaining width with 36px gutters. The header spans the whole area to the right of the sidebar. The conversation body scrolls independently above a bottom composer; the composer must not cover analysis or plan controls. For short windows, content scrolls instead of compressing body text.

## Color and geometry tokens

Color lock: **white means #FFFFFF**. Subtle raster texture in the generated reference is a generation artifact, not permission to tint the application canvas.

| Token | Value | Use |
| --- | --- | --- |
| canvas | #FFFFFF | Content, header, context rail, composer |
| sidebar | #111D2B | Persistent navigation |
| sidebar-selected | #253548 | Selected tender row |
| sidebar-text | #F6F8FA | Wordmark, current tender, primary controls |
| sidebar-muted | #ACB7C4 | Inactive tenders and secondary labels |
| text | #172536 | Titles and body |
| text-muted | #697786 | Descriptions and labels |
| border | #D5DCE3 | Tab rule, dividers, controls |
| border-strong | #8795A6 | Composer boundary |
| source-surface | #EFF2F5 | Citation chip fill |
| source-text | #56667A | Citation text |
| accent | #117D76 | Primary buttons, selected tab, checks |
| accent-hover | #0B6963 | Interactive hover |
| accent-soft | #E6F3F0 | Quiet selected state on white |
| warning | #E5A315 | Needs-review dot |
| danger | #B54741 | Errors and destructive text |
| focus | #117D76 | 2px outline with 3px offset |

Spacing scale: 4, 8, 12, 16, 24, 32, 36, 40, 48px.

Button radius 5px; citation radius 4px; composer radius 8px; selected sidebar row radius 3px. Default controls have 1px borders. Only the composer may have a barely perceptible shadow: 0 2px 8px rgba(17,29,43,.025). No gradients.

## Typography

Use **Inter** if available locally; otherwise Segoe UI, Arial, sans-serif. Bundle a chosen font if possible instead of requiring a network font at runtime. Keep weights clear and avoid monospace except exact file metadata, codes, and numeric amounts where useful.

| Role | Target size / line height | Weight |
| --- | --- | --- |
| Quantix wordmark | 34 / 40px | 650 |
| Project h1 | 42 / 48px | 650 |
| Project subtitle | 18 / 26px | 400 |
| Main section heading | 23 / 29px | 600 |
| Context section heading | 20 / 27px | 600 |
| Body | 16 / 24px | 400 |
| Main navigation/button | 16 / 22px | 500–600 |
| Plan row title | 16 / 22px | 600 |
| Muted caption and source chip | 15 / 21px | 400 |
| Small status | 14 / 20px | 400 |

At a minimum supported desktop width around 1100px, collapse the context rail behind a Project details button rather than squeezing the prose. Around 900px, allow collapsing the tender rail into a drawer while retaining current tender identity and New tender. Small windows keep at least 14px body and 36px control targets. Native desktop layout is the primary design.

## Component and icon inventory

Use lucide-react icons with 1.6–1.8 stroke, rounded caps/joins, and currentColor. Default 18–20px; document rows 28px. Do not invent a pictorial logo: Quantix is a text wordmark.

| Element | Icon / behavior |
| --- | --- |
| New tender | Plus, 20px, left of label in outlined rail button |
| Add files | Upload, 20px, left of label in outlined button |
| Tender Manager marker | ClipboardList, 24px inside a 38px outlined 5px-radius square |
| Analysis finding | Check, 20px teal; only show when actual analysis supports the finding |
| Source chip | Text-only compact button with real locator |
| Document row | File, 28px, text aligned in the next column |
| Settings | Settings, 26px, uncontained beside label |
| Composer attachment | Paperclip, 24px |
| Send | Send, 22px, white in 46px square teal button |
| Close drawer/modal | X, 20px |
| Back/source previous | ChevronLeft, 18px |
| Source next | ChevronRight, 18px |
| Errors | CircleAlert, 18px with clear text |
| Import | Upload, 28px in the import area; no illustration |

Buttons: primary teal filled; secondary white with slate outline; text action teal. Hover slightly darkens primary and adds a pale background to quiet actions. Focus is visible. Disabled controls use disabled semantics, muted text and lowered contrast, and a nearby reason if the action is blocked.

Tabs: text labels, 44px high, 30–36px separation. Active Manager has teal text and a 3px bottom border; inactive text is dark slate. Active state is conveyed semantically as well as by color.

Tender list: one vertical list, 48px row height, names truncate with a native title affordance. Selected row has a 4px teal leading indicator and lighter navy fill. No badges unless they communicate a real status essential to navigation.

Analysis: open prose rows, not a grid of cards. Each finding may have an inline or next-line group of real source chips. Each source chip must open the actual document and locator.

Plan: one compact ruled table/list. Number, short task title, plain-language description. Preserve 42–44px row heights when content fits; allow taller rows for actual content. Approve plan and Request changes are visible below the current proposal. Approved/running/failed/completed states replace actions with truthful status and relevant controls.

Context: open label/value pairs followed by ruled sections. File coverage lists actual files and actual extraction/review states. Open questions contains real unresolved questions or a neutral empty message.

Composer: multiline textarea in one bordered container. Attachment at lower left and send at lower right. Enter sends; Shift+Enter creates a new line if this is the implemented product convention. During a run, show a clear Stop control if cancellation is supported. Never show a successful send or completed analysis before the underlying operation succeeds.

## Visible copy lock for the populated visual reference

Sidebar: Quantix; New tender; Tenders; Riverside Works; North Depot; Eastbank Drainage; Settings; Saved on this device.

Header: Riverside Works; Civil works · Tender preparation; Add files; Manager; Files; Work; Estimate.

Manager: Tender Manager; Ready for review; I've reviewed the imported documents. Here is the scope and a proposed plan.

Document analysis:
- Earthworks, drainage and concrete works are included in the scope.
- Specification · p. 12; BOQ · Sheet 1.
- The bill of quantities is the starting point for pricing.
- BOQ · Sheet 1.
- The drawings need a quantity check before the estimate is prepared.
- Drawings · p. 3.

Proposed work plan; Review the plan before work starts.
1. Review scope — Confirm requirements and exclusions.
2. Check quantities — Compare the bill of quantities with the drawings.
3. Prepare estimate — Build the cost breakdown and record assumptions.
Approve plan; Request changes.

Context: Project scope; Work type; Civil works; Location; Not confirmed; Currency; EGP; Document coverage; Specification.pdf; Read; Bill of quantities.xlsx; Read; Drawings.pdf; Needs review; Open questions; Confirm the project location and pricing basis.

Composer: You stay in control of scope and approvals. Ask the manager or add an instruction…

These sample tender names, file names, findings, currency, sources, and plan entries are illustrative only. Replace all sample values with actual local data or explicitly empty states in the app. Never treat this copy lock as permission to fabricate evidence.

## Required states in the same visual system

### Empty office

Keep Quantix, New tender, Tenders, Settings, and local storage note in the sidebar. Show “No tenders yet” under Tenders. Main content has a modest 30px “Create your first tender” heading at x310, y200, one body line “Add the tender documents and work with the manager to prepare your submission.”, and a primary “New tender” button. Use an open surface with no large illustration, hero, or fake project rail.

### Create tender

A centered white modal, 520px wide, 24px padding, 8px radius, one subtle shadow and dimmed backdrop. Title “New tender”. Required field “Tender name”; optional fields are only shown when supported by the product. Footer Cancel and Create tender. Inline validation keeps the entered name intact. After success, select the new tender and show its empty manager state.

### Tender without files / import

Header shows the real tender name and the same tabs. Manager says “Add the tender documents to get started.” and a short line explaining that the manager will review them and propose a plan. Add files is the main action. Do not render the sample analysis or plan.

Files tab uses an open file list, a compact drop area with 1px dashed cool-gray border, “Drop files here or choose files”, and a real choose-files action. Supported type text comes from implemented import support. During import, show each selected filename with actual progress or a text state such as Importing, Reading, Ready, or Could not read. Unsupported or failed files retain a row with the specific problem and an action to remove or retry if supported. A failed extraction must not become “Read”.

An existing tender's Add files button opens the same native file selection/import flow. Successful import updates the real file list and scope coverage. Use a small in-context status message; avoid a celebratory modal.

### Manager analysis and plan states

Before analysis: explain the next action plainly with “Analyze documents” if the manager needs an explicit trigger. While processing: show actual current activity such as “Reading Specification.pdf…” when available; otherwise “Reviewing documents…”. Keep source files accessible. Do not use fake percent progress.

A proposed plan uses the reference table. Approval is an intentional action; specialist work starts according to the real product workflow. Request changes focuses the composer with a contextual instruction, or opens a small inline text field. After approval, display “Plan approved” and real work state. Errors remain visible with Retry if supported, with previous useful findings preserved.

When no source-backed findings exist, show a neutral explanation instead of green checks. When the provider/key is absent, the manager state links to Settings with a specific setup description; importing local documents can still be available if supported.

### Files tab

Same shell/header. Heading “Files”, count only when derived from actual files, and Add files. Use a ruled list/table with columns File, Status, and optional Size or Added only if actual metadata is available. Filename opens the source viewer. Removal uses a small confirmation only when necessary for actual data loss. Empty and import states use the same rules above.

### Work tab

Same shell/header. Heading “Work”. Display actual manager-created tasks as ruled rows with title, assignment/role when meaningful, and actual status. Selecting a task reveals its brief, dependencies, output and sources in an inline detail section or side drawer. No fabricated team presence, agent avatars, performance statistics, or activity. Empty message “No work has been planned yet.” with context pointing to the manager.

### Estimate tab

Same shell/header. Heading “Estimate”. A clean table accommodates the actual implemented estimate schema, normally description, unit, quantity, rate, and amount. Numeric values align right. Currency, totals, assumptions, exclusions and source links come from real data. Until there is a real estimate, show “No estimate yet” and “The manager will prepare an estimate after the scope and quantities are reviewed.” Do not prefill plausible amounts.

### Settings

A main-content settings page preserves the navy sidebar and its active Settings state. White canvas, maximum form width 700px, heading “Settings”. Organize actual supported configuration into open sections with plain headings such as “AI connection” and “Local files”. Show provider/model controls only if implemented and useful. Secret fields are masked, never exposed in normal display, and have an accessible show/hide button when supported. Use “Save settings”; actual connection testing uses a secondary “Test connection”. Show truthful saved, invalid, and test-failure states near the controls. Storage path has a read-only value and supported action such as “Open folder”. Never imply credentials or settings were saved before persistence succeeds.

### Source preview

Clicking a source chip or file opens a white source drawer anchored to the right, 620px wide at large desktops; expand it toward full width on smaller windows. Header shows the real filename, source location and Close. A compact toolbar supplies the actual available locator controls; PDF has page navigation, workbook has sheet selection and cell/range location, text has its supported locator. Prefer a real embedded preview where available. Otherwise show extracted source text and a clear “Open original file” action.

Selected cited text/range has a subtle teal highlight with a border or marker so color is not the sole cue. Preserve enough surrounding text for context. Source content scrolls; header stays visible. Missing locator or preview failure is explicit and still offers the original file where supported. Never manufacture a page image or citation.

## Accessibility and behavior

Use semantic headings, buttons, tables/lists, tabs and dialogs. All controls have accessible names. Focus follows modal/drawer opening and returns to the trigger on close. Escape closes non-destructive dialogs/drawers. Keyboard navigation works in tabs and tender selection. Keep readable contrast, persistent focus indicators, and status text alongside status colors. Respect reduced motion; normal transitions are simple 120–160ms opacity/background changes with no decorative movement.

## Design-only QA

- Generated with the built-in image_gen tool, not CLI.
- Selected output copied into this repository.
- Inspected with view_image after saving.
- Primary screen includes sidebar, full header/tabs, manager analysis, real-source-shaped citation controls, plan approval, context rail, and bottom composer.
- Typography, open paper container model, muted dividers, teal hierarchy, and navy selection have been inspected.
- The image contains no proprietary documents, real estimates, performance graphs or invented statistics.
- Intentional reference differences from the initial generation brief: image output is 1505 × 1045; generated sidebar is wider than the brief and is accepted at 274px at the target viewport; typography is larger than the brief and is recorded above; source chips and analysis remain fully readable. Raster background grain is excluded from implementation.
- No application implementation or browser fidelity claim is made by this design deliverable. The implementation owner should compare its current screenshot and this reference with view_image, checking sidebar width, main content alignment, type scale, rail width, row/control geometry and composer placement.

Exact generation prompt: [generation-prompt.txt](./generation-prompt.txt).

