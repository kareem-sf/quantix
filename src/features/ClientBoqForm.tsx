import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { tenderPath, useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/ui";
import { SourceDrawer, type SourceSelection } from "./Sources";

type MappingDraft = {
  key: number; sheet: string; headers: Schema<"Evidence">[]; itemIds: string[];
  rateColumn: string; amountColumn: string; writeQuantity: boolean; quantityColumn: string;
};
export type ClientBoqDraft = {
  artifactId: string; currency: string; taxBasis: "" | Schema<"ClientBoqInput">["tax_basis"];
  mappings: MappingDraft[]; mappingReviewed: boolean; quantityApproved: boolean;
};
export const emptyClientBoq = (): ClientBoqDraft => ({
  artifactId: "", currency: "", taxBasis: "", mappings: [],
  mappingReviewed: false, quantityApproved: false,
});

function validColumn(value: string) {
  return /^[A-Z]{1,3}$/.test(value) && [...value].reduce((total, letter) => total * 26 + letter.charCodeAt(0) - 64, 0) <= 16384;
}
export function clientBoqValue(value: ClientBoqDraft): Schema<"ClientBoqInput"> | null {
  if (!value.artifactId || !/^[A-Z]{3}$/.test(value.currency) || !value.taxBasis || !value.mappingReviewed || !value.mappings.length) return null;
  const ids = value.mappings.flatMap(mapping => mapping.itemIds);
  if (ids.length > 2000 || new Set(ids).size !== ids.length) return null;
  if (value.mappings.some(mapping => {
    const columns = [mapping.rateColumn, mapping.amountColumn, ...(mapping.writeQuantity ? [mapping.quantityColumn] : [])];
    return !mapping.sheet || !mapping.headers.length || !mapping.itemIds.length || mapping.itemIds.length > 1000 || columns.some(column => !validColumn(column)) || new Set(columns).size !== columns.length || (mapping.writeQuantity && !value.quantityApproved);
  })) return null;
  return {
    artifact_id: value.artifactId, currency: value.currency, tax_basis: value.taxBasis,
    mapping_reviewed: true, quantity_mapping_approved: value.mappings.some(mapping => mapping.writeQuantity) && value.quantityApproved,
    mappings: value.mappings.map(mapping => ({
      sheet: mapping.sheet, source_ids: mapping.headers.map(source => source.id), item_ids: mapping.itemIds,
      rate_column: mapping.rateColumn, amount_column: mapping.amountColumn,
      quantity_column: mapping.writeQuantity ? mapping.quantityColumn : null,
    } satisfies Schema<"ClientBoqMapping">)),
  };
}

function headerCells(source: Schema<"Evidence">) {
  const raw = source.metadata?.cells;
  if (!Array.isArray(raw)) return [];
  return raw.flatMap(cell => {
    if (!cell || typeof cell !== "object" || typeof cell.coordinate !== "string" || typeof cell.value !== "string" || !cell.value.trim() || cell.formula) return [];
    const match = /^([A-Z]{1,3})([1-9][0-9]*)$/.exec(cell.coordinate);
    return match ? [{ coordinate: cell.coordinate, column: match[1], row: Number(match[2]), text: cell.value }] : [];
  });
}
function sameQuantity(first: string | null, second: string | null) {
  const normalize = (value: string | null) => value === null ? null : value.replace(/^0+(?=\d)/, "").replace(/(\.\d*?)0+$/, "$1").replace(/\.$/, "");
  return normalize(first) === normalize(second);
}
function rowIssue(item: Schema<"EstimateItem">, mapping: MappingDraft, currency: string, taxBasis: ClientBoqDraft["taxBasis"]) {
  if (!item.confirmed) return "Source row needs confirmation.";
  if (item.unit_rate === null || item.rate_ex_vat === null || item.line_ex_vat === null) return "An approved price is required.";
  if (!item.quantity_cell || item.supplied_quantity === null) return "The original quantity and its source cell must be established.";
  if (item.effective_quantity === null) return "The quantity used for pricing is unresolved.";
  if (!currency || item.currency !== currency) return "The row currency must match the workbook currency.";
  if (item.tax_basis === "unknown" || item.vat_percent === null || (taxBasis === "including_vat" && item.line_inc_vat === null)) return "VAT treatment must be established.";
  const quantityColumn = item.quantity_cell.replace(/[0-9]+$/, ""), unitColumn = item.unit_cell.replace(/[0-9]+$/, "");
  if ([mapping.rateColumn, mapping.amountColumn].some(column => !!column && (column === quantityColumn || column === unitColumn))) return "Price columns would overwrite this row's unit or quantity.";
  if (mapping.writeQuantity && mapping.quantityColumn !== quantityColumn) return `This row's confirmed quantity column is ${quantityColumn}.`;
  if (!mapping.writeQuantity && !sameQuantity(item.effective_quantity, item.supplied_quantity)) return "An approved measured quantity differs. Map its quantity column explicitly.";
  return null;
}
function headersCoverMapping(mapping: MappingDraft, items: Schema<"EstimateItem">[], artifactId: string) {
  const rowNumbers = items.map(item => Number(item.quantity_cell?.match(/[0-9]+$/)?.[0])).filter(Number.isFinite);
  if (!rowNumbers.length) return false;
  const firstRow = Math.min(...rowNumbers);
  const columns = new Set(mapping.headers.filter(source => source.artifact_id === artifactId && source.sheet === mapping.sheet).flatMap(source => headerCells(source).filter(cell => cell.row < firstRow).map(cell => cell.column)));
  return [mapping.rateColumn, mapping.amountColumn, ...(mapping.writeQuantity ? [mapping.quantityColumn] : [])].every(column => columns.has(column));
}

export function ClientBoqForm({ tenderId, value, onChange, onReadyChange }: {
  tenderId: string; value: ClientBoqDraft; onChange: (value: ClientBoqDraft) => void;
  onReadyChange: (ready: boolean) => void;
}) {
  const base = tenderPath(tenderId);
  const artifacts = useResource<Schema<"Artifact">[]>(`${base}/artifacts`);
  const estimate = useResource<Schema<"EstimateView">>(`${base}/estimate`);
  const [source, setSource] = useState<SourceSelection | null>(null);
  const closeSource = useCallback(() => setSource(null), []);
  const workbooks = artifacts.data?.filter(artifact => artifact.is_current && /\.(xlsx|xlsm)$/i.test(artifact.name)) ?? [];
  const workbook = workbooks.find(artifact => artifact.id === value.artifactId);
  const workbookItems = estimate.data?.items.filter(item => item.artifact_id === value.artifactId) ?? [];
  const storedSheets = workbook?.metadata?.sheets;
  const metadataSheets = Array.isArray(storedSheets) ? storedSheets.flatMap(sheet => sheet && typeof sheet === "object" && typeof sheet.name === "string" ? [sheet.name] : []) : [];
  const sheets = [...new Set([...metadataSheets, ...workbookItems.map(item => item.sheet)].filter(Boolean))];
  const ready = !!clientBoqValue(value) && !!workbook && !!estimate.data && !estimate.data.refresh_required && !artifacts.error && !estimate.error && value.mappings.every(mapping => {
    const items = mapping.itemIds.map(id => workbookItems.find(item => item.id === id)).filter((item): item is Schema<"EstimateItem"> => !!item);
    return items.length === mapping.itemIds.length && items.every(item => item.sheet === mapping.sheet && !rowIssue(item, mapping, value.currency, value.taxBasis)) && headersCoverMapping(mapping, items, value.artifactId);
  });
  useEffect(() => onReadyChange(ready), [ready, onReadyChange]);
  useEffect(() => () => onReadyChange(false), [onReadyChange]);
  const change = (patch: Partial<ClientBoqDraft>) => onChange({ ...value, ...patch, mappingReviewed: false, quantityApproved: false });
  const updateMapping = (key: number, patch: Partial<MappingDraft>) => change({ mappings: value.mappings.map(mapping => mapping.key === key ? { ...mapping, ...patch } : mapping) });
  const writesQuantity = value.mappings.some(mapping => mapping.writeQuantity);
  return <>
    <div className="client-boq-form">
      <p className="muted">Create a priced copy in the client's original workbook layout. The supplied workbook stays preserved. Select only the rows and columns you have checked.</p>
      <ErrorNotice error={artifacts.error || estimate.error} />
      {artifacts.isPending || estimate.isPending ? <Loading>Loading supplied workbooks and current BOQ rows…</Loading> : null}
      <label>Client BOQ workbook<select required value={value.artifactId} onChange={event => change({ artifactId: event.target.value, mappings: [] })}><option value="">Choose a current XLSX or XLSM source</option>{workbooks.map(artifact => <option value={artifact.id} key={artifact.id}>{artifact.relative_path} · Version {artifact.version}</option>)}</select></label>
      {artifacts.data && !workbooks.length ? <p className="field-help">Import an XLSX or XLSM client workbook before creating a client-format copy.</p> : null}
      {workbook ? <button type="button" className="text-button" onClick={() => setSource({ artifactId: workbook.id, artifact: workbook })}>Inspect the original workbook</button> : null}
      <div className="form-grid"><label>Workbook currency<input required maxLength={3} pattern="[A-Z]{3}" value={value.currency} onChange={event => change({ currency: event.target.value.toUpperCase(), mappings: value.mappings.map(mapping => ({ ...mapping, itemIds: [] })) })} placeholder="Three-letter currency" /></label><label>Rate and amount tax basis<select required value={value.taxBasis} onChange={event => change({ taxBasis: event.target.value as ClientBoqDraft["taxBasis"] })}><option value="">Choose the workbook basis</option><option value="excluding_vat">Excluding VAT</option><option value="including_vat">Including VAT</option></select></label></div>
      <p className="field-help">These are the values to write. Check that the client's currency headings and tax notes agree with this basis; those headings stay as supplied.</p>
      {estimate.data?.refresh_required ? <p className="client-boq-warning">Source documents changed. Refresh the BOQ rows in Estimate before making this copy.</p> : null}
      {value.mappings.map((mapping, index) => <SheetMapping key={mapping.key} tenderId={tenderId} workbookId={value.artifactId} mapping={mapping} index={index} sheets={sheets} items={workbookItems} currency={value.currency} taxBasis={value.taxBasis} selectedElsewhere={value.mappings.filter(other => other.key !== mapping.key).flatMap(other => other.itemIds)} onChange={patch => updateMapping(mapping.key, patch)} onRemove={() => change({ mappings: value.mappings.filter(other => other.key !== mapping.key) })} onSource={setSource} />)}
      <button type="button" className="button" disabled={!workbook || value.mappings.length >= 100} onClick={() => change({ mappings: [...value.mappings, { key: Math.max(0, ...value.mappings.map(mapping => mapping.key)) + 1, sheet: "", headers: [], itemIds: [], rateColumn: "", amountColumn: "", writeQuantity: false, quantityColumn: "" }] })}>Add worksheet mapping</button>
      <p className="field-help">{value.mappings.reduce((count, mapping) => count + mapping.itemIds.length, 0)} selected rows. Selection does not establish complete BOQ coverage. Review the copied workbook and recalculated totals before approving a final export.</p>
      {writesQuantity ? <label className="checkbox-label client-boq-quantity-consent"><input type="checkbox" required checked={value.quantityApproved} onChange={event => onChange({ ...value, quantityApproved: event.target.checked })} />I explicitly approve writing the separately approved measured quantities into the mapped original quantity columns of this copy.</label> : <p className="field-help">Original quantities will be retained. Rate and amount mappings do not authorize quantity changes.</p>}
      <label className="checkbox-label"><input type="checkbox" required checked={value.mappingReviewed} onChange={event => onChange({ ...value, mappingReviewed: event.target.checked })} />I checked the workbook currency, tax notes, selected rows, source headers and every mapped column.</label>
    </div>
    {source ? createPortal(<SourceDrawer key={"sourceId" in source ? source.sourceId : source.artifactId} tenderId={tenderId} selection={source} artifacts={artifacts.data ?? []} onClose={closeSource} />, document.body) : null}
  </>;
}

function SheetMapping({ tenderId, workbookId, mapping, index, sheets, items, currency, taxBasis, selectedElsewhere, onChange, onRemove, onSource }: {
  tenderId: string; workbookId: string; mapping: MappingDraft; index: number; sheets: string[];
  items: Schema<"EstimateItem">[]; currency: string; taxBasis: ClientBoqDraft["taxBasis"];
  selectedElsewhere: string[]; onChange: (patch: Partial<MappingDraft>) => void; onRemove: () => void;
  onSource: (source: SourceSelection) => void;
}) {
  const [filter, setFilter] = useState("");
  const rows = items.filter(item => item.sheet === mapping.sheet);
  const selected = rows.filter(item => mapping.itemIds.includes(item.id));
  const visible = rows.filter(item => `${item.description} ${item.locator}`.toLowerCase().includes(filter.trim().toLowerCase()));
  const missingHeaders = mapping.itemIds.length > 0 && !headersCoverMapping(mapping, selected, workbookId);
  return <fieldset className="client-boq-mapping"><legend>Worksheet mapping {index + 1}</legend>
    <label>Worksheet<select required value={mapping.sheet} onChange={event => { setFilter(""); onChange({ sheet: event.target.value, headers: [], itemIds: [], rateColumn: "", amountColumn: "", writeQuantity: false, quantityColumn: "" }); }}><option value="">Choose a source worksheet</option>{sheets.map(sheet => <option key={sheet}>{sheet}</option>)}</select></label>
    <div className="form-grid"><label>Rate column<input required pattern="[A-Z]{1,3}" maxLength={3} value={mapping.rateColumn} onChange={event => onChange({ rateColumn: event.target.value.toUpperCase() })} placeholder="Column letters" /></label><label>Amount column<input required pattern="[A-Z]{1,3}" maxLength={3} value={mapping.amountColumn} onChange={event => onChange({ amountColumn: event.target.value.toUpperCase() })} placeholder="Column letters" /></label></div>
    <label className="checkbox-label"><input type="checkbox" checked={mapping.writeQuantity} onChange={event => onChange({ writeQuantity: event.target.checked, quantityColumn: "", itemIds: [] })} />Write approved measured quantities into this worksheet copy.</label>
    {mapping.writeQuantity ? <><label>Actual source quantity column<input required pattern="[A-Z]{1,3}" maxLength={3} value={mapping.quantityColumn} onChange={event => onChange({ quantityColumn: event.target.value.toUpperCase() })} placeholder="Confirmed quantity column" /></label><p className="field-help">This must be the selected rows' existing quantity column. The measurement must already be approved in Estimate. Changing this setting clears the row selection.</p></> : null}
    {mapping.sheet ? <>
      <label>Find rows in this worksheet<input value={filter} onChange={event => setFilter(event.target.value)} placeholder="Search description or source range" /></label>
      <p className="field-help">{mapping.itemIds.length} selected of {rows.length} identified rows. Rows with unresolved source, price or VAT decisions cannot be selected.</p>
      <div className="client-boq-row-picker" role="group" aria-label={`Rows in worksheet mapping ${index + 1}`}>
        {visible.map(item => {
          const reason = selectedElsewhere.includes(item.id) ? "Already selected in another mapping." : rowIssue(item, mapping, currency, taxBasis);
          return <div className="client-boq-row" key={item.id}><label className="checkbox-label"><input type="checkbox" checked={mapping.itemIds.includes(item.id)} disabled={!mapping.itemIds.includes(item.id) && (!!reason || mapping.itemIds.length >= 1000)} onChange={event => onChange({ itemIds: event.target.checked ? [...mapping.itemIds, item.id] : mapping.itemIds.filter(id => id !== item.id) })} /><span><strong>{item.description}</strong><span className="field-help">{item.locator} · {item.unit} · Supplied {item.supplied_quantity ?? "unresolved"} · Used {item.effective_quantity ?? "unresolved"}</span><span className="field-help">Rate {item.unit_rate ?? "not priced"} {item.currency}</span>{reason ? <span className="client-boq-warning">{reason}</span> : null}</span></label><button type="button" className="text-button" onClick={() => onSource({ sourceId: item.source_id })}>Inspect row source</button></div>;
        })}
        {!visible.length ? <p className="field-help">No source rows match this worksheet and filter.</p> : null}
      </div>
      {mapping.itemIds.some(id => !visible.some(item => item.id === id)) ? <p className="field-help">Some selected rows are outside this filter.</p> : null}
      <HeaderEvidencePicker key={`${workbookId}:${mapping.sheet}`} tenderId={tenderId} workbookId={workbookId} sheet={mapping.sheet} selected={mapping.headers} onChange={headers => onChange({ headers })} onSource={onSource} />
      {missingHeaders ? <p className="client-boq-warning">Select header passages above the chosen rows that identify every rate, amount and mapped quantity column.</p> : null}
    </> : null}
    <button type="button" className="text-button" onClick={onRemove}>Remove worksheet mapping {index + 1}</button>
  </fieldset>;
}

function HeaderEvidencePicker({ tenderId, workbookId, sheet, selected, onChange, onSource }: {
  tenderId: string; workbookId: string; sheet: string; selected: Schema<"Evidence">[];
  onChange: (sources: Schema<"Evidence">[]) => void; onSource: (source: SourceSelection) => void;
}) {
  const [offset, setOffset] = useState(0);
  const evidence = useResource<Schema<"Evidence">[]>(`${tenderPath(tenderId)}/artifacts/${workbookId}/evidence?offset=${offset}&limit=50`);
  const candidates = evidence.data?.filter(source => source.sheet === sheet && headerCells(source).length > 0) ?? [];
  return <section className="client-boq-headers"><h4>Source column headers</h4><p className="field-help">Choose the actual source rows containing the mapped column headings. The register includes all sheets; only passages from this worksheet are shown.</p>
    {selected.length ? <div className="client-boq-selected-headers">{selected.map(source => <div key={source.id}><button type="button" className="source-chip" onClick={() => onSource({ sourceId: source.id })}>{source.locator}</button><button type="button" className="text-button" onClick={() => onChange(selected.filter(item => item.id !== source.id))}>Remove header reference</button></div>)}</div> : null}
    <ErrorNotice error={evidence.error} />{evidence.isPending ? <Loading>Loading worksheet source passages…</Loading> : null}
    <div className="client-boq-header-options">{candidates.map(source => <label className="checkbox-label" key={source.id}><input type="checkbox" checked={selected.some(item => item.id === source.id)} disabled={!selected.some(item => item.id === source.id) && selected.length >= 50} onChange={event => onChange(event.target.checked ? [...selected, source] : selected.filter(item => item.id !== source.id))} /><span><strong>{source.locator}</strong><span className="field-help">{headerCells(source).map(cell => `${cell.coordinate}: ${cell.text}`).join(" · ")}</span></span></label>)}</div>
    {evidence.data && !candidates.length ? <p className="field-help">No header text from this sheet appears in this part of the source register.</p> : null}
    <div className="inline-actions"><button type="button" className="text-button" disabled={offset === 0 || evidence.isPending} onClick={() => setOffset(value => Math.max(0, value - 50))}>Earlier source passages</button><button type="button" className="text-button" disabled={evidence.isPending || (evidence.data?.length ?? 0) < 50} onClick={() => setOffset(value => value + 50)}>More source passages</button></div>
  </section>;
}
