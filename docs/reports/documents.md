# Document worker implementation

Implemented the `Segment`, `Extraction`, `extract_document`, and one-based `render_pdf_page` contracts in `backend/quantix/documents.py`. Optional legacy Word conversion is isolated in `backend/quantix/document_word.py`. No persistence, API, UI, shared model, configuration, or customer-source files were changed by this task.

## Evidence and behavior

- PDFium extracts embedded page text with full Unicode support and exact page locators. The preview endpoint returns bounded PNG bytes. Extraction and rendering share one process-wide PDFium lock; cancellation also works while waiting for that lock. Empty/unreadable text pages, text-decoding problems, partial coverage, and parser failures are explicit.
- Paired read-only openpyxl readers preserve each populated source cell's coordinate, value, formula, stored result, number format, type, error, and hidden-column context. Each row retains hidden-row/sheet and merged-range context. Workbook metadata records VBA presence; no macro execution or formula calculation occurs. Stored results are explicitly unverified. Quantity/unit headers are not reinterpreted. Actual worksheet XML bounds prevent a wrong declared range from hiding source cells. Parser warnings that indicate omitted content are attached to the affected workbook.
- DOCX extraction retains ordered paragraphs and table-cell locators, nested tables, headers, and footers. Text boxes, tracked changes, embedded document chunks, and unextracted notes generate coverage warnings. Locators are structural; no page numbers are invented.
- Genuine OLE `.doc` inputs can be converted through installed Microsoft Word. The service first copies the source into a disposable directory. A separate hidden worker force-disables macros, disables automatic link updates, opens read-only without adding recent files, saves a separate DOCX, closes without saving, and restores settings. Conversions are serialized; timeout and cancellation terminate the worker and the specifically identified new Word process. Cleanup verifies process creation time and executable name. A process that predates the worker is never controlled or terminated. Conversion failure or inability to establish exclusive process ownership returns `unsupported` with a visible reason. Converted source locators are explicitly identified as referring to converted DOCX structure.
- DWG `ACxxxx` headers and RTF content are recognized. DWG and RTF conversion remain explicit coverage exceptions. Binary `.xls`, `.xlsb`, and unrelated formats are not silently treated as supported OOXML.

## Limits

Source file limit: 256 MiB. Office package expansion: 128 MiB total, 32 MiB/member, 10,000 members. PDF extraction: 2,000 pages. Workbook extraction: 20,000 rows total, 256 columns/sheet, 500,000 visited cells. Evidence output: 12 MiB, 50,000 segments, 200,000 characters/segment. Warnings are capped with an additional-warning count. Word conversion: 32 MiB input/output and 60 seconds. PNG rendering: 4,096 pixels/side and 12 million pixels. XML entity declarations and encrypted ZIP members are rejected. Limits preserve whole source values; skipped content is reported rather than silently truncated.

## Verification

Generated-fixture red/green cycles cover page provenance, actual PNG dimensions, formulas/caches/errors, reversed quantity/unit data, hidden rows/sheets/columns, merged cells, nested Word tables/headers, malformed formats, ZIP/XML guards, cancellation, page/row/output limits, wrong worksheet dimensions, parser warnings, and empty documents. Windows-boundary tests use controlled subprocesses and COM doubles; ordinary tests never open Office or customer files.

Command: `backend/.venv/Scripts/python.exe -m pytest backend/tests/test_documents.py backend/tests/test_document_word.py -q` from the repository root. Latest targeted result before handoff: **23 passed**. Owned files also pass Ruff lint and formatting checks.

Separate read-only acceptance checks on the supplied package verified:

- The complete 583-page specification produced 583 sourced page segments; a drawing page produced text and a PNG preview.
- All nine BOQs extracted their source rows and all 128 formula cells. One civil workbook correctly reports two spreadsheet errors. An architectural workbook's malformed print header/footer exposed an openpyxl omission warning; a generated regression test now verifies that this becomes a document coverage warning.
- A DOCX vendor document produced 510 structural segments.
- A genuine binary DOC converted successfully through installed Word 16.0.20326.20132 and produced 1,413 structural segments. No WINWORD process remained after the successful conversion.
- The checked DWG was correctly left as unsupported CAD pending a converter.
- SHA-256 values of every source inspected in these checks remained unchanged. No source or extracted customer text was committed or uploaded.

## Remaining limitations

There is no OCR engine installed in the checked standard locations. Image text and geometric interpretation are not indexed by this increment. PDF/Word text extraction is not a checked quantity takeoff. Spreadsheet cached values are not proven current. Word conversion depends on installed desktop Office and its local startup/add-in/protection environment; source files requiring interaction stay as explicit exceptions. Conversion and rendering preserve access to evidence but do not establish commercial or engineering approval.

Validated libraries: Python 3.12, pypdfium2 5.13.0, openpyxl 3.1.5, python-docx 1.2.0, Pillow 12.3.0, and Windows pywin32. PyMuPDF is not used. Relevant upstream contracts: [PDFium text/render and concurrency](https://pypdfium2.readthedocs.io/en/stable/python_api.html), [openpyxl read modes](https://openpyxl.readthedocs.io/en/stable/api/openpyxl.reader.excel.html), [Word macro security](https://learn.microsoft.com/en-us/office/vba/api/word.application.automationsecurity), [Word read-only opening](https://learn.microsoft.com/en-us/office/vba/api/word.documents.open).
