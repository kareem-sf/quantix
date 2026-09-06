# Supplier quotation correspondence

Implemented 2026-09-06. No real SMTP connection, message submission, IMAP login or inbox access was performed. Tests use synthetic workspaces, fake keyring storage and explicit test-only mail server doubles; default test network constructors raise rather than contact a server.

## Files and root integration

- `backend/quantix/correspondence.py`: QuoteService, secure mail clients, local delivery receipts and reply evidence registration.
- `backend/quantix/correspondence_models.py`: strict request/response schemas.
- `backend/quantix/correspondence_routes.py`: create_router(repo).
- `backend/tests/test_correspondence.py`: behavior, transport and backup rollback regressions.
- This report. No root API, repository, UI, settings, job, backup or office files were changed.

Root includes create_router(repo) under existing authenticated `/api` middleware and generates frontend types from OpenAPI. Router creation invokes recover_interrupted_sends once at application startup; a database row left in `sending` becomes `uncertain`, never automatically retried. QuoteService construction alone does not change active sending states, so read-only worker callers can create service instances safely.

No dependency was added. Production uses stdlib email, smtplib, imaplib, ssl, sqlite3 and the already installed keyring package.

## Routes and payloads

All routes below have `/api` prefix:

| Method and route | Payload or response |
| --- | --- |
| GET `/tenders/{tid}/quotes` | list[QuoteRecord] |
| POST `/tenders/{tid}/quotes` | DraftInput -> QuoteRecord |
| PATCH `/tenders/{tid}/quotes/{id}` | Full replacement DraftInput -> QuoteRecord |
| GET `/tenders/{tid}/quotes/{id}/preview` | QuotePreview |
| GET `/tenders/{tid}/quotes/{id}/eml` | Real message/rfc822 attachment |
| POST `/tenders/{tid}/quotes/{id}/approve` | SendDecision -> QuoteRecord; no network |
| POST `/tenders/{tid}/quotes/{id}/send` | SendDecision -> QuoteRecord; actual SMTP submission if all gates pass |
| GET `/tenders/{tid}/quotes/{id}/replies` | list[ReplyRecord] |
| POST `/tenders/{tid}/quotes/{id}/replies` | ManualReply -> ReplyRecord and Tender evidence |
| GET `/mail/settings` | Public MailSettings |
| PATCH `/mail/settings` | MailSettingsPatch; passwords write-only |
| POST `/mail/sync` | `{max_messages:30}` -> SyncResult; maximum 50 per request |

DraftInput: `{to:[plain_email],cc:[],subject,body,attachment_ids:[],source_ids:[]}`. Recipients use plain ASCII addresses; display-name syntax and header control-character injection are rejected. The body is retained with canonical line endings. Attachments are saved Artifact IDs in the selected Tender, never caller-provided filesystem paths. Exact versions, names, hashes and sizes are displayed in the preview. Total attachments are limited to 20 MiB.

QuotePreview includes quote,sender,attachments,fingerprint,smtp_host,smtp_ready,warnings. Its fingerprint binds current recipients, body, subject, sender/SMTP account, Message-ID, draft date, source references and exact attachment hashes/version metadata. Editing clears approval. Account/source changes invalidate the fingerprint. Old source versions remain explicit in preview warnings. Sent previews retain the original submitted sender and EML even if account settings change later.

SendDecision: `{fingerprint,engineer_confirmed:true,rationale,restore_reconciliation?:string}`. Approval and sending both require the current fingerprint and explicit engineer consent. The optional restore reconciliation note becomes mandatory for restored older drafts where later delivery cannot be established. The UI must ask the engineer to check the sent mailbox or supplier and record that the request was not already submitted; never automatically generate or silently fill this note.

Mail account public fields: smtp_host,smtp_port,smtp_security (`ssl|starttls`),smtp_username,from_address,imap_host,imap_port,imap_username,imap_mailbox. Password fields smtp_password/imap_password are SecretStr inputs and never returned or stored in SQL. An empty password clears the corresponding stored credential. The OS keyring namespace is separate from OpenAI credentials and scoped by workspace home. `smtp_ready` and `imap_ready` mean configuration/credential presence, not a verified live connection.

ManualReply: `{sender,received_at:timezone_aware_ISO_datetime,subject?,text,headers?,source_ids?:[]}`. ReplyRecord returns newly registered reply evidence in `source_ids`; original optional links are retained separately in `supporting_source_ids`. Other fields include origin,sender,received_at,date_header,date_basis,subject,text,headers,message_id,warnings. For IMAP, received_at is the local retrieval timestamp, date_basis is `retrieved_at`, and the original message Date header is preserved separately. Manual dates use `engineer_entered`.

QuoteService methods mirror the routes: create_draft,edit_draft,list_drafts,get,preview,eml,approve,send,register_reply,replies,settings,update_settings,sync_replies,recover_interrupted_sends. Root's future Manager tools should expose read-only methods and source IDs; approval/sending remain engineer actions.

## Submission integrity and restore safety

The Message-ID is generated at draft creation and retained. Before any SMTP connection, the exact outbound EML and `sending` state are committed, and an external delivery attempt is durably reserved. SMTP SSL or STARTTLS uses ssl.create_default_context with certificate/hostname validation and minimum TLS 1.2. Authentication occurs only after TLS. There is no insecure fallback or automatic retry.

`sent` means SMTP accepted all recipients, not confirmed inbox delivery. `partially_sent` records refused recipient addresses/codes. Explicit sender/data/all-recipient refusal or a connection/authentication failure before submission produces `failed`. A lost acknowledgement after submission begins produces `uncertain`, and blind resend is prohibited. A cleanup/QUIT error does not erase an already reported acceptance.

`home/mail-delivery.sqlite` is intentionally outside the restorable main database and archive payload set. Its append-only delivery_receipts table uses SQLite WAL/FULL durability and immutable UPDATE/DELETE triggers. Receipts record stable Message-ID, content fingerprint, sender, attempt/result phase and any reconciliation note. They are committed before network and checked on every send. A previous sent, partial, uncertain or unfinished attempt blocks resend even if an older main database says approved/unsent.

Local draft-creation receipts also bind the completed restore-history IDs present when a draft was created. An older restored draft requires fresh reconciliation; a newly created draft after restore does not. This comparison uses restore identity, not imported database timestamps or clock ordering. On another computer, missing newer receipt history cannot establish non-delivery, so reconciliation is mandatory for restored drafts.

QuoteRecord exposes `delivery_history_status` and `delivery_history_fingerprint`. **After rollback, root UI/Manager must preserve this newer local fact separately from the main database's historical `status`.** A different fingerprint means restored wording is not proved identical to the recorded submission. Do not label an older main record as definitely unsent merely because its historical state is approved. Do not overwrite or restore the external delivery ledger during normal workspace recovery.

This is conservative duplicate-submission prevention, not exactly-once SMTP. Sending outside Quantix, deleting local receipts, or copying an incomplete workspace outside the supported restore flow cannot be fully reconciled automatically. An interrupted local write can leave a conservative pending receipt despite no actual SMTP submission; it remains blocked pending external checking/new deliberate correspondence.

## Reply sync and evidence

IMAP uses SSL with verified certificates, a read-only mailbox selection, UID-based search and BODY.PEEK fetches. It fetches headers/size first, matches exact generated IDs in In-Reply-To or References, and never guesses from a subject or ID substring. Multiple ambiguous matches are not assigned automatically. BODY.PEEK uses a bounded partial request; the public IMAP read method is also wrapped to reject oversized server literals before allocation.

Each request checks at most 50 messages and each imported raw reply is limited to 2 MiB. Oversized messages remain in the mailbox and produce a visible manual-import warning. The cursor and duplicate identity include mailbox host/user/folder, UIDVALIDITY and UID. A changed UIDVALIDITY starts a fresh scan. `more_available` reports a remaining batch; initial scans proceed from older UIDs toward current mail. No flags are intentionally changed and no mailbox writes/expunge occur.

Raw matched EML and manual-registration JSON are preserved both locally in SQLite and as content-addressed original objects. Repository.register_artifact creates a Tender Artifact, and reply text is split into source segments of at most 6,000 characters with sender/date/message/quote provenance. Every returned reply source ID resolves through repo.get_evidence in its Tender. Standard workspace backup includes these referenced originals and SQL records automatically.

HTML body text can be converted without rendering scripts; formatting loss is explicit. Remote attachments remain inside the preserved original EML and are visibly marked as not imported for engineering analysis. No attachment code/macro executes. A reply does not accept a commercial rate, approve a quantity, resolve an assumption or release an output. Matching a Message-ID is thread correlation, not proof of sender identity or commercial authority.

## Verification

```powershell
backend/.venv/Scripts/python.exe -m pytest backend/tests/test_correspondence.py -q
backend/.venv/Scripts/ruff.exe check backend/quantix/correspondence.py backend/quantix/correspondence_models.py backend/quantix/correspondence_routes.py backend/tests/test_correspondence.py
```

Result: **22 tests passed; lint passed.** One existing upstream Starlette/AnyIO TestClient deprecation warning remains.

Covered: account-free EML export, attachment integrity/version binding, foreign attachments, header injection, password redaction/keyring storage, SSL and STARTTLS validation, persisted pre-network state, partial recipients, authentication failure, uncertainty/retry prohibition, original sent preview, manual/long reply evidence, read-only IMAP matching and UID deduplication, oversized-message handling, restart recovery, public schemas, same-home restore after sent/uncertain/interrupted submission, immutable receipt storage, and cross-home reconciliation with clock-independent new-draft handling. Backup regression calls use the root's current expected_sha256 restore interface.

Validated primary APIs: [smtplib](https://docs.python.org/3.12/library/smtplib.html), [imaplib](https://docs.python.org/3.12/library/imaplib.html), [EmailMessage](https://docs.python.org/3.12/library/email.message.html), [TLS defaults](https://docs.python.org/3.12/library/ssl.html#ssl.create_default_context). Configured SMTP/IMAP password authentication must be supported by the chosen mail provider; OAuth account flows and remote attachment extraction are outside this increment.
