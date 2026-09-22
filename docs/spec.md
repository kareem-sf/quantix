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

- **Tender Manager.** One per tender. Leads the work, hires and briefs staff, reviews their output and brings
  decisions to the engineer. The engineer can adjust its personality.
- **Staff.** Hired by the Manager for this tender's actual needs, with a generated name, role, discipline,
  experience, background, temperament, speaking style and professional opinions, and a locally drawn portrait.
  They speak in their own voice, disagree, raise concerns and push back on the Manager or the engineer.
- **Conversation.** The engineer talks to the Manager, messages any staff member directly, and can read the team
  room where staff and the Manager brief, ask, hand over and review each other's work.
- **Presence.** Each person shows their current activity, drawn from what they are really doing.
- **Autonomy.** Default: the office works through to a finished tender and stops at gates and for real questions.
  Settings → Fully autonomous: the office approves its own gates, and each of those decisions is marked "office-approved,
  not reviewed". Export and release always stay with the engineer.

## Gates (engineer in the loop)

Scope and BOQ structure · Method of measurement · Quantities · Rates and build-ups · Subcontract and supplier
choices · Markups and final price · Release.

## Modules (MVP)

1. **Documents.** Import a tender package and keep the originals unchanged. Read PDF, XLSX/XLSM, DOCX and scanned
   pages (OCR, Arabic included, in the correct reading order). Group the documents, search them by exact words and
   by meaning, and view them page by page. DWG files are listed and flagged as not readable yet.
2. **BOQ.** Import the client BOQ from Excel, PDF or Word with source references. Record the method of measurement
   the tender states.
3. **Quantity Take-Off.** Staff set and check each sheet's scale, then place measurements (length, area, count) as
   geometry on PDF drawings. Quantix computes the quantities. The engineer sees every measurement on the sheet and
   can edit or redo it, or measure by hand. Quantix compares the takeoff with the BOQ: matches, differs, missing
   from the BOQ, or not on the drawings.
4. **Estimating.** Per item, either a unit rate with a dated source or a first-principles build-up (labour, plant,
   material, subcontract, outputs, wastage). Then preliminaries, overheads, profit and tender adjustments, and a
   tender summary.
5. **Subcontract and supplier quotes.** Trade and material packages from the BOQ, enquiry drafts the engineer sends
   from their own mail program, imported quotes, and line-by-line levelling (missing prices filled with our rate,
   exclusions priced back in). Then a recommendation, the engineer's choice, and the chosen rates carried into the
   estimate.
6. **Submission.** A checklist of what the tender requires, each item with its source clause; drafted documents;
   the priced BOQ in the client's own format; and a local export package. Nothing is sent to a client.

## Company knowledge (persists across tenders)

A master resource and rate library, the subcontractor and supplier directory, past tenders for benchmarking, and
company rules and preferences (markups, standard exclusions and qualifications, house style). Every new team reads
them. Dated information is revalidated before reuse.

## AI

API keys for Anthropic, OpenAI, Google, xAI and one OpenAI-compatible endpoint. ChatGPT/Codex and Grok subscriptions
through their official clients only. The tender chooses its connection; the Manager may give staff other models
from the connections already allowed. There are no spending controls in the MVP; each agent turn has a step limit.

## Language and platform

English interface; the office reads and writes Arabic where the tender needs it. A desktop app on Windows first,
built so a hosted web version with several users can follow.

## Later (not in the MVP)

Tender programme, cash flow, DWG reading, Arabic interface, web version and firm accounts, AI spending controls,
supplier email sending.

## Done when

- A synthetic tender goes end to end through every gate: priced BOQ, levelled subcontract package, submission
  checklist and export.
- The Arch-Civil Rev03 reference package is fully read, its drawings are taken off with visible measurements and
  its BOQ is priced; the engineer spot-checks a sample. The package stays private and is never committed.
- Service tests, UI tests, typecheck and lint pass in CI.
