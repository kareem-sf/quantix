# Architecture

## Shape: a modular monolith

One developer, a local-first desktop app now and a hosted web version later. So Quantix is one Python service with
clear domain modules, one React interface and a thin desktop shell. No microservices, plugin system or
compatibility layers.

```
ui/        React + Vite + TypeScript, Tailwind + shadcn/ui, TanStack Query; API types generated from OpenAPI
desktop/   Tauri 2 shell: owns the window and native file dialogs. No domain logic.
service/   Python 3.12, FastAPI, SQLAlchemy 2 + Alembic
  quantix/
    core/        settings, database, ids, event stream (SSE), the data home ~/.quantix
    ai/          connections: Pydantic AI for API keys; Codex/Grok official clients in a worker via a local MCP bridge
    documents/   import, hashing, PDF / Excel / Word readers, OCR, Arabic reading order, search
    office/      Manager and staff runtime, hiring and personas, portraits, tasks, messages, scheduler, gates
    boq/ takeoff/ estimate/ subcontract/ submission/
    company/     rate library, directory, past tenders, company rules
    api/         HTTP routes per module
  tests/
docs/
```

**Why:**

- Python has the strongest PDF, OCR, Excel and multi-provider agent libraries.
- SQLAlchemy and Alembic keep SQLite now and Postgres later as a configuration change.
- Server-sent events give live office updates to both the desktop and the web version.

Each domain module owns its models, its service functions and the agent tools that call them.

## Boundaries

- AI connections and their keys are kept in `~/.quantix/auth.json`, and office settings (office mode, the office's
  AI) in `~/.quantix/settings.json`: plain JSON files the engineer can read. The service never returns a key.
- The UI talks only to the HTTP API on loopback. Each launch of the service gets a fresh bearer token. In
  development the Vite server starts the service on a free port and forwards `/api` to it with the token, so the
  browser preview and the desktop window use the same path and the UI never holds the token or a key.
  Release packaging, which embeds the service in the desktop app, comes later.
- Search by meaning runs on the engineer's computer: a small multilingual model, fetched once into
  `~/.quantix/models` and checked against pinned digests. No document text leaves the computer for it.
- OCR runs on the engineer's computer too (`quantix/documents/ocr.py`, RapidOCR on ONNX Runtime). Once a document
  is read, a background worker reads each page without text of its own, one at a time, at 200 dpi up to an A3
  sheet. The English model comes with the package. The Arabic model, which also reads the English on an Arabic
  page, is fetched once into `~/.quantix/models/ocr` and checked against its pinned digest. A document whose own
  text is at least 30% Arabic is read in Arabic; one with no text of its own tries its first page both ways and
  keeps the better. The words go into the page's text (rows of the page, cells split by `|`), the keyword index
  and then the meaning index, with `pages.ocr` (the model, `empty` or `failed`) and `pages.ocr_score`. The same
  file in another tender is read once. `read_page` says when a page was read by OCR and how sure it was.
- Web research sends only search words and page addresses: to Firecrawl (without a key, or with the engineer's)
  and to TinyFish when Firecrawl can't answer and the engineer has added its key. Pages read are saved in the
  database, so what the office cites from them can be checked.
- Every record belongs to a tender, and every tender belongs to an owner, so accounts can be added later.
- Long work is kept in the database, not only in memory. After a restart, interrupted work resumes or is shown as
  stopped (see Turns below).

## The office runtime

- **One runtime, many agents.** The Manager and staff differ only in persona and tools. Personas are generated
  by the Manager's `hire` tool.
- **Tools per person** (`quantix/office/packs.py`). Each turn gets a small core: talking, looking up records and
  pages, `calculate`, and for the Manager his review and delegation tools. The rest come in packs, one per kind of
  work (documents, boq, takeoff, pricing, subcontract, submission): pydantic-ai capabilities carrying their tools
  and a short method. `hire` records the kinds of work a person does (`profile.work`), and those packs are loaded
  every turn; the others show only their name and one line until the person calls `load_capability`, and calling
  a hidden tool tells them which pack to load. Staff hired before packs do every kind of work. The Manager's packs
  hold only the tools that check work, never ones that produce records; his hiring tools stay loaded while the
  team has fewer than 2 staff. An AI that can't read images gets no image tools, and its instructions say so. No
  tool depends on the model searching for it, so it works on every provider and keeps the prompt cache stable.
- **Inboxes.** Engineer messages, direct messages, team-room posts, task assignments and answers land in an
  agent's inbox. A scheduler wakes agents with pending inbox items and runs one turn each: a tool loop with a step
  limit. API connections run several agents at once; a subscription client runs one at a time. Stop cancels
  everything on the tender.
- **Turns.** Each turn is a row in `turns` (`TurnRecord`), written as it starts and filled in as it ends:
  - who took it and on which AI model;
  - each tool call, with Quantix's reason when it sent the call back;
  - the model requests and tokens used;
  - how it ended: done, out of steps, a tool that kept failing, an AI failure, stopped, interrupted when Quantix
    closed, or failed.

  The office resumes from these. Someone whose last turn was cut short or interrupted carries on without a new
  message, and the turn budget counts the turns since the engineer last wrote. A stopped or paused office is
  kept on the tender (`office_paused`), so it stays stopped after a restart. The conversation of a turn cut short
  is kept in memory for the next turn; after a restart the person carries on from the records instead. The rows
  also explain the work. They are never shown in the work areas.
- **AI allowance and scorecard.** `tender_allowance` in `settings.json` caps the tokens (read and written, from
  `turns`) a tender's office may use. Before each turn the scheduler pauses the tender once it has used them, with
  a plain notice in the team room. `quantix/review/scorecard.py` reads `turns` and the review outcomes of the records
  filed in each turn: a record belongs to the filer's latest turn that started before it. `GET /ai/usage` serves both
  to Settings.
- **Real conversation.** Agents talk only through `message_engineer`, `post_to_team` and `raise_concern`. The team
  room is exactly those records. A reply to the engineer (they wrote last) carries its `sources`, each something the
  sender opened, or `next_steps`: up to 3 open at a time, each becomes the sender's own "Follow up" task. An open
  task given, sent back or set since someone's last turn began wakes them once. Someone who ends a turn with open
  tasks, having filed nothing and told no one, gets a plain notice from Quantix in the team room, which wakes the
  Manager to find out why; Quantix's notices never wake the staff they name. The chat shows the sources as
  links (`messages.sources`, the same shape as a decision's sources).
- **Looking up the office's work.** `quantix/review/lookup.py` opens any record by its reference or BOQ line: what
  it says and rests on, with Quantix's figures, who made and decided it, the checks while it is undecided, and
  every other version of the same work with why it was sent back. A checklist item whose draft was sent back points
  to that draft, so whoever redrafts it corrects it rather than starting again. It also finds records by words and
  lists the priced BOQ a page at a time. Agents reach it through `open_record`, `find_records` and `priced_boq`.
- **What was opened.** `opened` keeps each page and record a person opened (their tools write it), and the summaries
  they called. It is what their sources are checked against.
- **The package as the office reads it.** `quantix/review/package.py` gives a spreadsheet's rows a part at a time,
  the changes between copies of a document, the package map (staff set a document's kind and summary, shown as its
  group and description) and coverage kept apart: pages with their own text, scans read by OCR and still to read,
  pages the office opened and pages its work cites. The documents list serves the last three to the engineer.
- **Proposals, review and gates.** Agents never write domain records. Staff propose through each module's tools,
  which validate and recompute every number, and the record starts as `proposed`: in the Tender Manager's review
  queue (`quantix/review`). The Manager has no production tools. He accepts a record (`reviewed`, waiting at the
  engineer's gate; or `office_approved` in Fully autonomous mode) or sends it back (`rejected`, with the correction
  posted in the team room naming who made it). New proposals wake him. The engineer approves or sends back reviewed
  records, and can reopen approved ones. The statuses and review fields live in `quantix/core/review.py`.
- **Checks.** `quantix/review/checks.py` computes findings from the records whenever they are asked for; they are
  never stored. Each finding is a blocker or a warning, with its message and the pages or records it rests on. The
  Manager can't accept a record with a blocker. A warning needs his reason, kept in `acceptances` under the
  finding's key, which names the check and the records, so a changed record is checked again. The thresholds are
  named constants that each message states: plausibility checks, not standards.
- **Escalations.** The Manager's `escalate` tool turns a record in his queue into a `Decision` with its subject
  (`subject_kind`, `subject_id`), its sources (document pages and BOQ lines, checked to exist) and his suggested
  corrections as the options. A record has at least one source and one open escalation at most. While it is
  open, he can't decide the record. Once the same work has been sent back twice, he can't send it back again
  until the engineer has answered an escalation about it. The answer reaches his chat through the usual
  decision answer.
- **Audit.** `quantix/review/audit.py` runs the tender-level checks and the record checks on work past the
  Manager's review. The engineer's approval settles a warning but not a blocker. For approved work it asks only for
  blockers, so the drawing's geometry isn't read. The client-row check finds the quantity and unit columns from
  the rows already entered. The Manager accepts audit warnings by a short name (a hash of the key) through
  `audit_tender`. `GET /tenders/{id}/audit` serves the Overview. The build endpoint fills the package's not-ready list
  from the audit's blockers.
- **Lessons.** A `Verdict` may carry a `lesson`, kept in `lessons` (`quantix/review/lessons.py`), only for work that
  needed correcting: a send-back, or the accepted version of work sent back before. Quantix turns away a lesson the
  office already follows, one already among the company rules, or one like a lesson the engineer dropped. That test
  is the same word overlap as for repeated questions. Every briefing on the tender lists the tender's lessons. The
  engineer keeps one (`PATCH /lessons/{id}`), which copies it into `company_rules` under a topic from the record's
  kind, or drops it.
- **Evidence.** A cited location must exist and must have been opened by whoever cites it. The tools that file
  BOQ lines, facts, scales, measurements, quoted rates, quotes, checklist items and pricing columns check each cited
  page against `opened`, as `message_engineer` does for its sources.
- **Figures on request.** `quantix/core/calculate.py` works out arithmetic (a whitelist of operators over named
  values) and cut and fill on a grid of levels. `quantix/estimate/analysis.py` gives where the money is, what a
  change would do to the price without saving anything, and how a rate compares with the library (flagging entries
  over 12 months old), earlier tenders, similar lines and quotes. `quantix/review/activity.py` searches the
  conversation, says what changed since the engineer last wrote, and runs the checks on someone's own waiting work.
  A library suggestion is a decision (`subject_kind` `library`); answering "Keep it in the library" saves the rate.
- **Newer copies.** A changed file with the same path replaces the older copy (`replaced`). As the reader saves
  the newer copy, in the same transaction, `quantix/review/revisions.py` moves the work that cites an older copy
  onto it wherever what the work cites is unchanged. That means:
  - the quoted words, on the same page or on the one page that has them;
  - a workbook row with the same cells, when rows were added or taken out above it;
  - the drawing within 20 points of a measurement or a scale line.

  The document's note says how much moved. Work left on an older copy is a blocker in the record checks and in the
  audit, and a newer copy that leaves some wakes the Manager, whose briefing lists it. Staff redo it from the
  newer copy. A BOQ line is revised in place, so its rate and measurements stay with it. A checklist item takes
  the new clause and goes back to the Manager. A scale or measurement on the newer copy replaces the older copy's.
  A fact, or a rate priced from a quote, may replace an approved one that rests on an older copy.

## The submission package

- **One content model.** `submission/content.py` holds a document as blocks: headings, paragraphs of bold and plain
  runs, bulleted and numbered lists with levels, and tables with totals rows. The office writes drafts in Markdown,
  which `from_markdown` (markdown-it-py) reads into blocks.
- **Where the content comes from.** `submission/package.py` builds the documents from Quantix's records:
  - approved drafts;
  - the work programme, laid out from its stored durations;
  - the priced BOQ, grouped by the client's bill.
- **Renderers.** Word and PDF are drawn from the same blocks, in one house style:
  - `word.py` (python-docx): styles with no theme fonts or colours, the letterhead in the header, "Page X of Y"
    fields, numbered lists that restart, and tables with fixed widths and a repeating header row;
  - `pdf.py` (fpdf2): Noto Sans with Noto Sans Arabic as the fallback font, shaped with HarfBuzz; the combined PDF
    adds a cover and a contents page with page numbers;
  - `workbooks.py` (openpyxl): formulas, number formats and print setup;
  - `deck.py` (python-pptx): the internal tender summary.
- **Letterhead.** `company.profile(home)` reads the details from `settings.json` and the logo from
  `~/.quantix/company/logo.png`.
- **Package layout.** Documents, Correspondence and Internal folders. Names are capped so paths stay under Windows'
  260 characters.

## Quantity take-off

Sheets are rendered and their vector paths extracted. Agents detect or set the scale, find elements with vision,
and place geometry in page coordinates, snapped to vectors where they exist. Quantix computes the length, area or
count from the geometry and scale. Measurements link to BOQ items, and Quantix computes the comparison.

## CAD drawings (DWG and DXF)

- **The reader.** `cad/` is a Rust crate that builds `qx-dwg` on two MPL-2.0 libraries, opencadcodec (reading) and
  opencadkernel (exact lengths, areas and closed regions), pinned to the revisions OpenCADStudio ships. OpenCADStudio
  itself is GPL: its ideas are used, never its code. The service runs `qx-dwg` as a separate process with a time
  limit (`documents/cad.py`), so a drawing the young library can't cope with only stops that process.
  `QUANTIX_CAD` points at another build; otherwise the service uses `cad/target/release` or `debug`.
- **What it writes.** `qx-dwg read` walks model space, then each paper layout that holds anything, placing every
  block reference's contents with its full transform (nested blocks, MINSERT arrays, base points, object normals).
  Layer "0" inside a block takes the reference's layer, as CAD shows it; an anonymous copy of a dynamic block is
  counted under its definition. It never counts from the library's `entities()`, which includes block definitions.
  Each placed object gets a key (the block references it sits in, then its handle, e.g. `1F3/2A`), its layer, block,
  flags (closed, annotation, inside a block), extent, and exact length and area in drawing units. The folder,
  `tenders/<id>/drawings/<sha256>/` beside the stored file, holds `drawing.json` (units, spaces, layers, blocks,
  xrefs, what couldn't be read), `objects.json` (keys, texts, block references with attributes, dimensions with
  their written text, tables, hatches, viewports) and three little-endian arrays. A drawing is read once; a newer
  reader format reads it again. A file the failsafe reader returns empty is unreadable.
- **Pages.** Each space is a page: model space first, then the layouts. A page's text is the words printed there,
  top to bottom, then tables as `A1=… | B1=…` and dimensions whose text was written by hand, so search, quotes and
  citations work unchanged. The page's width and height are its extent in drawing units.
- **Units.** A drawing's units are a `Scale` on page 1, in metres per drawing unit, proposed by staff and approved
  by the engineer. The office may only set what the drawing's header states; the engineer may set anything.
  `units_evidence` gives the header, the extent in metres and notes that name units.
- **Measurements by rule.** `measure_drawing` takes a `cad.Rule` (layers, blocks, types, words, block attributes,
  closed outlines, hatch pattern, region, rooms, or keys). Quantix resolves it to the keys of the objects it takes
  when it is filed (`Measurement.entities`, with `rule` kept; `points` stays empty), so the Manager reviews exactly
  what was counted. `quantity()` computes from those objects' lengths, areas or copies and the drawing's units each
  time it is read. A rule that takes nothing, linked to a BOQ line, records that the work isn't on the drawing:
  `compare()` gives `not_on_drawings`.
- **The layer map** (`LayerMap`, `takeoff/layers.py`) says what layers and blocks are, from a closed list
  (`drawings.MEANINGS`). It is worked out on one drawing and applies by name to every drawing; a newer map overrides
  an older one name by name, an approved map before one still being decided. Its checks refuse names no drawing has
  and warn where the drawing contradicts a meaning (walls with no parallel faces a wall's thickness apart, doors
  with no swing or block, room outlines that aren't closed).
- **Rooms** (`drawings.rooms`): closed outlines on room-outline layers, or else the regions the walls, fire-rated
  walls, columns and windows close off, found by opencadkernel's `bounded_faces` (`qx-dwg faces`). A door's leaf and
  swing are left out and its opening closed by the swing's radius that isn't the leaf. Regions under 1 m² or thinner
  than 0.5 m are dropped. Each room takes the names printed inside it (the room-name layers, or else any word).
- **Checks** (`drawings.drawing_problems`, `boq_problems`, `grid_problems`), computed and never stored: what
  couldn't be read (3D solids, images, proxies, undecoded records, unloaded xrefs, clipped blocks), layers that
  don't print but hold objects, lines drawn twice, written dimensions that disagree with the drawn length, drawn
  work the map calls work that no measurement takes, room names in no closed room, services crossing fire-rated
  walls, grids that differ between drawings, and the BOQ's own lines billed twice, provisional and prime cost sums,
  measured lines without a quantity, odd units and lines not on the drawings. Drawing measurements get their own
  record checks: objects no longer in the drawing (blocker), the same object measured twice for a BOQ line
  (blocker), the same sheet measured in another format, and objects on layers that don't print. The audit warns on
  what couldn't be read in drawings the takeoff rests on.
- **Newer copies.** A drawing measurement moves onto a newer copy only when its rule takes exactly the same keys
  there, each with the same extent, length and area; an object added that the rule takes keeps it on the older copy.
  Units move when the header's units are the same.
- **Tender queries** (`TenderQuery`, `review/queries.py`): missing items, conflicts, BOQ errors and clarifications,
  each with sources (a page with its quoted words, or a drawing's objects), an optional BOQ line and measurements,
  which document governs, and the wording for the client. Quantix checks every source when it is raised, and warns
  on figures no source or takeoff gives, a query already raised, a BOQ line that may cover a missing item, a
  conflict that doesn't say which document governs, and a takeoff that matches the BOQ. The contract type and the
  order of precedence are tender facts, audited once there are queries.
- **Office tools** (the `drawings` pack): `drawing_overview`, `query_drawing`, `view_drawing` (the only one that
  needs an AI that reads images) and `find_problems` read; `set_drawing_units`, `measure_drawing`,
  `propose_layer_map` and `raise_query` produce.
- **Screens.** `GET /documents/{id}/pages/{n}/screen` sends a page packed as `QXD1` (segments in 32-bit floats about
  the page's centre, each segment's object, each object's layer, type, flags and extent, and the texts). The
  Takeoff screen draws it with WebGL, with texts and highlights on a canvas over it; clicking picks the nearest
  object, and `choose` gives Quantix's count, length and area of what is chosen. The Queries screen lists tender
  queries and layer maps for the engineer's decision, and what the checks find.

## Testing

Service tests use pytest. Agent behaviour is tested with a scripted model at the provider boundary (Pydantic AI
`FunctionModel`), so the real runtime, tools, gates and database are exercised. UI tests use Vitest and Testing
Library. CI runs tests, typecheck, Ruff and Clippy on every pull request.
