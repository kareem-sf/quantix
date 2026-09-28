# Quantix specification

Quantix is a tendering office for a construction engineer. It covers what RIB Candy's Estimating, Quantity
Take-Off and Subcontract adjudication modules do, but the work is done by AI staff: a Tender Manager heads a team
it hires for each tender. The engineer reviews, decides and steers.

Written 23 September 2026 from interviews with the product owner.

## Who it is for

A construction engineer or estimator preparing tenders. They think in BOQ items, drawings, rates and quotes. They
live in spreadsheets and PDFs, work under deadline, and must be able to defend every number. They want the team
to do the work, not a tool to operate.

## Experience principles

1. **What needs me?** One place collects everything waiting for the engineer, most urgent first. It's always one
   click away and always shows a count.
2. **Every number has a source.** Quantities open their measurement on the drawing, rates open their build-up or
   quote, and findings open the page they came from. It's one click, with nothing to search for.
3. **Quiet by default.** A neutral screen with one accent colour. Colour means something (waiting, differs,
   approved), never decoration. Details come on demand.
4. **Tables behave like spreadsheets.** Keyboard movement, copy and paste to Excel, units always shown, figures
   right-aligned in tabular numerals.
5. **The team is present, not noisy.** Faces and one line each show who is doing what. The engineer talks to the
   Manager or to anyone directly, and can read the team room when they want to.
6. **Quick, informed decisions.** Each decision shows the proposal next to its evidence, with Approve, Reject and
   Ask. Similar items can be approved together.
7. **Nothing moves under the cursor.** Live updates appear gently and never reorder what the engineer is reading.
8. **Plain language.** Construction English, not AI or software jargon.

## The office

- **Tender Manager.** One per tender. Leads the work, hires and briefs staff and brings decisions to the
  engineer. He never produces records himself: everything his staff propose comes to his review queue first, and
  he accepts it, saying what he checked, or sends it back, saying what to correct. Only what he accepted reaches
  the engineer's gates. Quantix checks every record before he decides: he can't accept one with a blocker, and he
  accepts a warning only with his reason, which the engineer sees beside the finding and its source. What the
  office can't settle, such as work sent back twice without a fix, he escalates to the engineer. An escalation
  shows the problem, where it shows (document pages and BOQ lines), what Quantix found and his suggested
  corrections. The engineer chooses one or answers in their own words, and the Manager applies the answer. Before
  he tells the engineer the tender is ready, he runs Quantix's audit of the whole tender and clears it. The engineer
  can adjust his personality.
- **Audit before release.** Quantix audits the whole tender:
  - work still waiting for the Manager or the engineer, and open decisions;
  - missing tender facts or markups, and BOQ lines with a quantity but no rate;
  - rows of the client's BOQ workbook that have a quantity but aren't in the BOQ;
  - documents it couldn't read, and a checklist that isn't ready;
  - every open finding on the office's work.

  The Overview shows the audit under "Before release", the build button counts its blockers, and the built package
  lists them as not ready.
- **Lessons.** When work needed correcting, the Manager can state the lesson with his review: one general rule
  that stops the same mistake elsewhere. The whole office on the tender follows it from then on, including people
  hired later. The Overview lists the lessons under "What the office learned". The engineer keeps one as a company
  rule for later tenders, or drops it, and the office won't learn a dropped lesson again.
- **Staff.** Hired by the Manager for this tender's actual needs, with a generated name, role, discipline,
  experience, background, temperament, speaking style and professional opinions, and a locally drawn portrait.
  They speak in their own voice, disagree, raise concerns and push back on the Manager or the engineer.
- **Conversation.** The engineer talks to the Manager, messages any staff member directly, and can read the team
  room where staff and the Manager brief, ask, hand over and review each other's work.
- **Presence.** Each person shows their current activity, drawn from what they are really doing.
- **Autonomy.** Default: the office works through to a finished tender and stops at gates and for real questions.
  Settings → Fully autonomous: the Tender Manager's review approves the gates, and each of those decisions is marked
  "approved by the office, not reviewed by you"; the engineer can reopen any of them with a reason. Export and release
  always stay with the engineer.

## Gates (engineer in the loop)

Scope and BOQ structure · Method of measurement · Quantities · Rates and build-ups · Subcontract and supplier
choices · Markups and final price · Release.

## Modules (MVP)

1. **Documents.** Import a tender package and keep the originals unchanged. Read PDF, XLSX/XLSM, DOCX and scanned
   pages (OCR, Arabic included, in the correct reading order). Group the documents, search them by exact words and
   by meaning, and view them page by page. CAD drawings (DWG and DXF) are read too: each space (model space, then
   each layout) is a page of the words printed there, and every object is kept with its layer, block and exact
   geometry, 3D solids with their volume. Clipped block references show only what their clip boundary shows, and
   the drawings a drawing refers to (xrefs) are placed in it from the package. What couldn't be read in a drawing
   (images, drawings it refers to that the package lacks) is said plainly. A newer
   copy of a file, such as an addendum's revised bill or drawing, replaces the older one. The office's work moves
   onto the newer copy wherever what it cites is unchanged there, and Quantix says how much moved. The rest must be
   done again from the newer copy, and holds the release until it is.
2. **BOQ.** Import the client BOQ from Excel, PDF or Word with source references. Record the method of measurement
   the tender states.
3. **Quantity Take-Off.** Staff set and check each sheet's scale, then place measurements (length, area, count) as
   geometry on PDF drawings. Quantix computes the quantities. The engineer sees every measurement on the sheet and
   can edit or redo it, or measure by hand. On CAD drawings there is no scale to calibrate: staff propose the
   drawing's units from what it states, and take off by rule from the drawing's own objects (count a block, the
   length or area of what is on a layer, a room's area and perimeter, the volume of 3D solids or their weight by a
   density); the engineer can click objects and measure them. Staff propose a layer map (what each layer and block is), which the engineer approves; Quantix finds the
   rooms from it. Quantix compares the takeoff with the BOQ: matches, differs, missing from the BOQ, or not on the
   drawings.
   Quantix's own checks find what to look into: lines drawn twice, written dimensions that disagree with the
   drawing, drawn work nothing measures, room names outside any closed room, services crossing fire-rated walls,
   doors, windows and fittings drawn as loose lines instead of their block,
   grids that differ between drawings, and in the BOQ lines billed twice, provisional sums and lines with no
   quantity. Staff raise **tender queries** for work drawn or specified but not billed, documents that disagree and
   errors in the BOQ, each with its sources and which document governs under the tender's order of precedence (a
   tender fact, with the contract type). The engineer decides which queries go to the client.
4. **Estimating.** Per item, either a unit rate with a dated source or a first-principles build-up (labour, plant,
   material, subcontract, outputs, wastage). Then preliminaries, overheads, profit and tender adjustments, and a
   tender summary.
5. **Subcontract and supplier quotes.** Trade and material packages from the BOQ, enquiry drafts the engineer sends
   from their own mail program, imported quotes, and line-by-line levelling (missing prices filled with our rate,
   exclusions priced back in). Then a recommendation, the engineer's choice, and the chosen rates carried into the
   estimate.
6. **Submission.** A checklist of what the tender requires, each item with its source clause; drafted documents;
   the priced BOQ in the client's own format; and a local export package. Nothing is sent to a client. The package
   carries the firm's letterhead from Company details:
   - one submission PDF, with a cover, contents and every document;
   - the priced BOQ, as a workbook with formulas and as a PDF, bill by bill with subtotals and VAT;
   - each document, in Word and PDF, with real headings, lists and tables;
   - the client queries, kept apart to be sent by the engineer's own email;
   - the checklist and a tender summary deck for the firm's bid review, marked internal.

## Company knowledge (persists across tenders)

A master resource and rate library, the subcontractor and supplier directory, past tenders for benchmarking, and
company rules and preferences (markups, standard exclusions and qualifications, house style), including the
lessons the engineer kept. Every new team reads them. Dated information is revalidated before reuse.

**Web research.** For market facts the documents don't give (material and plant prices, suppliers,
subcontractors, datasheets, outputs), the office searches the web in general words, never the client's or the
project's name. Quantix saves every page the office reads, as it was then, and checks a quote from it the way it
checks a quote from a document. A web price is a dated market price, not a quote: it goes through the same review
and gate, with a note on how it becomes the rate. Search works without a key; free keys in Settings add more
searches and a backup service.

Each firm is in the directory once. Quantix knows a firm by its name whatever the case, punctuation, Arabic letter
forms or legal form ("ABC Contracting Co. W.L.L." is ABC Contracting), so a quote under another spelling is the
same firm's. A name that may be a firm already there is added only once the adder says it is a different firm.
When a firm was entered twice anyway, the engineer merges the entries: its enquiries and quotes move to one, which
keeps the other name so it finds the firm from then on.

## AI

API keys for Anthropic, OpenAI, Google, xAI and one OpenAI-compatible endpoint. ChatGPT/Codex and Grok subscriptions
through their official clients only. The tender chooses its connection; the Manager may give staff other models
from the connections already allowed. Each agent turn has a step limit. In Settings the engineer can set an AI
allowance per tender, in tokens: the office pauses on a tender once it has used it. Settings also shows how each
AI model has done in the office: turns finished, tool calls Quantix sent back, work accepted and tokens per
accepted record. No provider, model or token details appear in the work areas.

## Language and platform

English interface; the office reads and writes Arabic where the tender needs it. A desktop app on Windows first,
built so a hosted web version with several users can follow.

## Later (not in the MVP)

Tender programme, cash flow, Arabic interface, web version and firm accounts, AI spending in money rather than
tokens, supplier email sending. For CAD drawings: views of 3D models (a solid is measured from its shape and drawn
as seen from above), front and back clipping, and checking inverted clips against a drawing that has one (they are
read as the boundary the file stores).

## Done when

- A synthetic tender goes end to end through every gate: priced BOQ, levelled subcontract package, submission
  checklist and export.
- The Arch-Civil Rev03 reference package is fully read, its drawings are taken off with visible measurements and
  its BOQ is priced; the engineer spot-checks a sample. The package stays private and is never committed.
- Service tests, UI tests, typecheck and lint pass in CI.
