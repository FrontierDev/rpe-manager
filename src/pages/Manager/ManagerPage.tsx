import { useState, type ChangeEvent } from "react";
import { Panel } from "../../../core/ui/Panel";
import type { ManagerConfiguration, WowInstallation } from "../../models/configuration";
import { getDatasetRows, type DatasetRow } from "../../models/datasets";
import type { SelectedWowInstallationDiscovery } from "../../models/local-discovery";
import type { SelectedProtocolState } from "../../models/protocol-state";
import type { WowInstallationProductChoice } from "../../models/discovery";
import type { RpeUpdateState } from "../../models/rpengine-update";
import type { WowModificationSafetyState } from "../../models/processes";

interface ManagerPageProps {
  configuration: ManagerConfiguration | null;
  discovery: SelectedWowInstallationDiscovery | null;
  rpeUpdateState: RpeUpdateState | null;
  rpeOperation: boolean;
  rpeOperationError: string | null;
  safetyState: WowModificationSafetyState | null;
  onRpeOperation: () => void;
  protocolState: SelectedProtocolState | null;
  isLoading: boolean;
  isSelecting: boolean;
  errorMessage: string | null;
  productChoices: WowInstallationProductChoice[] | null;
  onChooseFolder: () => void;
  onSelectInstallation: (path: string) => void;
  onSelectProduct: (path: string) => void;
  onToggleAccount: (accountId: string, selected: boolean) => void;
  onQueueRuleset: (payload: string) => Promise<string>;
  onQueueDataset: (payload: string) => Promise<string>;
  onRefresh: () => void;
  manualDatasetDetails: Record<string, { name: string; category: string }>;
  onQueueDatasetRemoval: (row: DatasetRow) => Promise<string>;
}

export function ManagerPage(props: ManagerPageProps) {
  const [showRulesetImport, setShowRulesetImport] = useState(false);
  const [rulesetText, setRulesetText] = useState("");
  const [rulesetMessage, setRulesetMessage] = useState<string | null>(null);
  const [isQueueingRuleset, setIsQueueingRuleset] = useState(false);
  const [showDatasetImport, setShowDatasetImport] = useState(false);
  const [datasetText, setDatasetText] = useState("");
  const [datasetMessage, setDatasetMessage] = useState<string | null>(null);
  const [isQueueingDataset, setIsQueueingDataset] = useState(false);
  const selectedInstallation = props.configuration?.installations.find((item) => item.id === props.configuration?.selectedInstallationId) ?? null;
  const selectedAccounts = props.discovery && props.configuration
    ? props.configuration.selectedAccountIds[props.discovery.installation.id] ?? [] : [];
  const rows = getDatasetRows(props.protocolState?.accounts ?? [], props.manualDatasetDetails);

  return <section className="page-section">
    <h1>Manager</h1>
    {props.errorMessage ? <p className="discovery-error">{props.errorMessage}</p> : null}
    <Panel className="manager-selectors">
      {props.rpeOperation ? <div className="rpe-download-progress" role="status"><strong>Downloading RPEngine {props.rpeUpdateState?.latestVersion ?? "release"}</strong><p>Download and installation are in progress.</p></div> : <>
      <label className="selector-row">World of Warcraft installation:
        <select value={selectedInstallation?.path ?? ""} onChange={(event) => props.onSelectInstallation(event.target.value)} disabled={props.isLoading || props.isSelecting}>
          <option value="">Select an installation…</option>
          {props.configuration?.installations.map((installation) => <InstallationOption key={installation.id} installation={installation} />)}
        </select>
      </label>
      <button className="secondary-button" type="button" onClick={props.onChooseFolder} disabled={props.isSelecting}>{props.isSelecting ? "Checking…" : "Choose folder"}</button>
      <label className={`selector-row account-selector${props.discovery === null ? " account-selector-disabled" : ""}`}>Account(s):
        <details className="account-dropdown">
          <summary>{selectedAccounts.length === 0 ? "No accounts selected" : `${selectedAccounts.length} account${selectedAccounts.length === 1 ? "" : "s"} selected`}</summary>
          <div className="account-select-list">
            {props.discovery?.accounts.length ? props.discovery.accounts.map((account) => <label key={account.id}><input type="checkbox" checked={selectedAccounts.includes(account.id)} onChange={(event: ChangeEvent<HTMLInputElement>) => props.onToggleAccount(account.id, event.target.checked)} /> {account.id}</label>) : <span>None available</span>}
          </div>
        </details>
      </label>
      <div className="rpengine-version"><span>RPE: {props.rpeUpdateState?.local.version ?? props.discovery?.rpengine.version ?? "Not detected"}</span>{props.rpeUpdateState?.status === "current" ? <span>Up to date</span> : null}{props.rpeUpdateState?.status === "update_available" ? <><span>Update required</span><button className="primary-button" type="button" onClick={props.onRpeOperation} disabled={props.safetyState?.canModifyWowFiles !== true}>Update RPEngine</button></> : null}{props.rpeUpdateState?.status === "check_failed" ? <span className="discovery-error">Update check failed</span> : null}</div>
      </>}
    </Panel>
    {props.rpeOperationError ? <p className="discovery-error">{props.rpeOperationError}</p> : null}
    {props.productChoices ? <Panel className="product-choice-panel"><strong>Choose a World of Warcraft product</strong><div className="product-choice-list">{props.productChoices.map((choice, index) => <button className={index === 0 ? "primary-button" : "secondary-button"} key={choice.path} type="button" onClick={() => props.onSelectProduct(choice.path)}>{choice.product.toUpperCase()}{index === 0 ? " (default)" : ""} — {choice.path}</button>)}</div></Panel> : null}
    <section className="dataset-section" aria-labelledby="datasets-heading">
      <div className="section-heading"><h2 id="datasets-heading">Datasets</h2><div className="import-actions"><button className="secondary-button" type="button" onClick={props.onRefresh}>Refresh</button><button className="secondary-button" type="button" onClick={() => setShowDatasetImport((visible) => !visible)}>Import Dataset</button><button className="secondary-button" type="button" onClick={() => setShowRulesetImport((visible) => !visible)}>Import Ruleset</button></div></div>
      {showDatasetImport ? <Panel className="ruleset-import"><label>Dataset export<textarea value={datasetText} onChange={(event) => setDatasetText(event.target.value)} placeholder="Paste the full RPE_DATASET_V1 export" /></label><div><button className="primary-button" type="button" disabled={isQueueingDataset} onClick={() => { setIsQueueingDataset(true); setDatasetMessage(null); void props.onQueueDataset(datasetText).then((message) => { setDatasetMessage(message); if (message.startsWith("Queued")) setDatasetText(""); }).catch((error: unknown) => setDatasetMessage(error instanceof Error ? error.message : "Dataset could not be queued.")).finally(() => setIsQueueingDataset(false)); }}>{isQueueingDataset ? "Queueing…" : "Install"}</button>{datasetMessage ? <span className="status-detail">{datasetMessage}</span> : null}</div></Panel> : null}
      {showRulesetImport ? <Panel className="ruleset-import"><label>Ruleset export<textarea value={rulesetText} onChange={(event) => setRulesetText(event.target.value)} placeholder="Paste the RPE ruleset export" /></label><div><button className="primary-button" type="button" disabled={isQueueingRuleset} onClick={() => { setIsQueueingRuleset(true); setRulesetMessage(null); void props.onQueueRuleset(rulesetText).then(setRulesetMessage).catch((error: unknown) => setRulesetMessage(error instanceof Error ? error.message : "Ruleset could not be queued.")).finally(() => setIsQueueingRuleset(false)); }}>{isQueueingRuleset ? "Queueing…" : "Queue import"}</button>{rulesetMessage ? <span className="status-detail">{rulesetMessage}</span> : null}</div></Panel> : null}
      <DatasetTable rows={rows} isLoading={props.isLoading} onRemove={props.onQueueDatasetRemoval} />
    </section>
  </section>;
}

function InstallationOption({ installation }: { installation: WowInstallation }) {
  return <option value={installation.path} disabled={installation.availability !== "available"}>{installation.product?.toUpperCase() ?? "CUSTOM"} — {installation.path}{installation.availability === "unavailable" ? " (unavailable)" : ""}</option>;
}

function DatasetTable({ rows, isLoading, onRemove }: { rows: DatasetRow[]; isLoading: boolean; onRemove: (row: DatasetRow) => Promise<string> }) {
  if (rows.length >= 0) return <div className="dataset-table-wrap"><table className="dataset-table"><thead><tr><th>Dataset</th><th>Category</th><th>Publisher</th><th>Installed revision</th><th>Available revision</th><th>Status</th><th>Action</th></tr></thead><tbody>
    {isLoading ? <tr><td colSpan={7}>Loading local state...</td></tr> : rows.length === 0 ? <tr><td colSpan={7}>No datasets installed.</td></tr> : rows.map((row) => <tr key={row.id}><td>{row.dataset}</td><td>{row.category}</td><td>{row.publisher}</td><td>{row.installedRevision ?? "-"}</td><td>{row.availableRevision ?? "-"}</td><td><span className={`dataset-status dataset-status-${row.status}`}>{row.status.replaceAll("_", " ")}</span></td><td>{row.status === "installed" && row.catalogueId && row.datasetId && row.hash ? <button className="link-button" type="button" onClick={() => { if (window.confirm(`Queue removal of ${row.dataset}?`)) void onRemove(row); }}>Remove</button> : <span className="status-detail">Managed locally</span>}</td></tr>)}
  </tbody></table></div>;

  // Retained temporarily below for source-history readability.
  return <div className="dataset-table-wrap"><table className="dataset-table"><thead><tr><th>Dataset</th><th>Category</th><th>Publisher</th><th>Installed revision</th><th>Available revision</th><th>Status</th><th>Action</th></tr></thead><tbody>
    {isLoading ? <tr><td colSpan={6}>Loading local state…</td></tr> : rows.length === 0 ? <tr><td colSpan={6}>No datasets installed. The catalogue will appear here when available.</td></tr> : rows.map((row) => <tr key={row.id}><td>{row.dataset}</td><td>{row.publisher}</td><td>{row.installedRevision ?? "—"}</td><td>{row.availableRevision ?? "—"}</td><td><span className={`dataset-status dataset-status-${row.status}`}>{row.status.replaceAll("_", " ")}</span></td><td><button className="link-button" type="button" disabled>Managed locally</button></td></tr>)}
  </tbody></table></div>;
}
