import { useState, type ChangeEvent } from "react";
import { Panel } from "../../../core/ui/Panel";
import type { ManagerConfiguration, WowInstallation } from "../../models/configuration";
import { getDatasetRows, type DatasetRow } from "../../models/datasets";
import type { SelectedWowInstallationDiscovery } from "../../models/local-discovery";
import type { SelectedProtocolState } from "../../models/protocol-state";
import type { WowInstallationProductChoice } from "../../models/discovery";
import type { RpeUpdateState } from "../../models/rpengine-update";
import type { WowModificationSafetyState } from "../../models/processes";
import type { CataloguePackage } from "../../models/catalogue";
import { CataloguePanel } from "./CataloguePanel";

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
  cataloguePackages: CataloguePackage[];
  catalogueLoading: boolean;
  catalogueError: string | null;
  onRefreshCatalogue: () => void;
  onQueueCataloguePackage: (pkg: CataloguePackage, password?: string) => Promise<string>;
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
      <RpeAddonAction updateState={props.rpeUpdateState} detectedVersion={props.discovery?.rpengine.version ?? null} safetyState={props.safetyState} onOperate={props.onRpeOperation} />
      </>}
    </Panel>
    {props.rpeOperationError ? <p className="discovery-error">{props.rpeOperationError}</p> : null}
    {props.productChoices ? <Panel className="product-choice-panel"><strong>Choose a World of Warcraft product</strong><div className="product-choice-list">{props.productChoices.map((choice, index) => <button className={index === 0 ? "primary-button" : "secondary-button"} key={choice.path} type="button" onClick={() => props.onSelectProduct(choice.path)}>{choice.product.toUpperCase()}{index === 0 ? " (default)" : ""} — {choice.path}</button>)}</div></Panel> : null}
    <CataloguePanel packages={props.cataloguePackages} localRows={rows} loading={props.catalogueLoading} error={props.catalogueError} protocolState={props.protocolState} selectedAccounts={selectedAccounts} controls={<><button className="secondary-button" type="button" onClick={props.onRefresh}>Refresh</button><button className="secondary-button" type="button" onClick={() => setShowDatasetImport((visible) => !visible)}>Import Dataset</button><button className="secondary-button" type="button" onClick={() => setShowRulesetImport((visible) => !visible)}>Import Ruleset</button></>} imports={<>{showDatasetImport ? <Panel className="ruleset-import"><label>Dataset export<textarea value={datasetText} onChange={(event) => setDatasetText(event.target.value)} placeholder="Paste the full RPE_DATASET_V1 export" /></label><div><button className="primary-button" type="button" disabled={isQueueingDataset} onClick={() => { setIsQueueingDataset(true); setDatasetMessage(null); void props.onQueueDataset(datasetText).then((message) => { setDatasetMessage(message); if (message.startsWith("Queued")) setDatasetText(""); }).catch((error: unknown) => setDatasetMessage(error instanceof Error ? error.message : "Dataset could not be queued.")).finally(() => setIsQueueingDataset(false)); }}>{isQueueingDataset ? "Queueing…" : "Install"}</button>{datasetMessage ? <span className="status-detail">{datasetMessage}</span> : null}</div></Panel> : null}{showRulesetImport ? <Panel className="ruleset-import"><label>Ruleset export<textarea value={rulesetText} onChange={(event) => setRulesetText(event.target.value)} placeholder="Paste the RPE ruleset export" /></label><div><button className="primary-button" type="button" disabled={isQueueingRuleset} onClick={() => { setIsQueueingRuleset(true); setRulesetMessage(null); void props.onQueueRuleset(rulesetText).then(setRulesetMessage).catch((error: unknown) => setRulesetMessage(error instanceof Error ? error.message : "Ruleset could not be queued.")).finally(() => setIsQueueingRuleset(false)); }}>{isQueueingRuleset ? "Queueing…" : "Queue import"}</button>{rulesetMessage ? <span className="status-detail">{rulesetMessage}</span> : null}</div></Panel> : null}</>} onRefresh={props.onRefreshCatalogue} onInstall={props.onQueueCataloguePackage} onRemove={props.onQueueDatasetRemoval} />
  </section>;
}

function RpeAddonAction({ updateState, detectedVersion, safetyState, onOperate }: {
  updateState: RpeUpdateState | null;
  detectedVersion: string | null;
  safetyState: WowModificationSafetyState | null;
  onOperate: () => void;
}) {
  const status = updateState?.status;
  const action = status === "not_installed"
    ? "Install RPEngine"
    : status === "damaged"
      ? "Repair RPEngine"
      : "Update RPEngine";
  const canOperate = status === "not_installed" || status === "damaged" || status === "update_available";

  return <>
    <div className="rpengine-version">
      <span>RPE: {updateState?.local.version ?? detectedVersion ?? "Not detected"}</span>
      {status === "current" ? <span>Up to date</span> : null}
      {status === "not_installed" ? <span>Not installed</span> : null}
      {status === "damaged" ? <span>Installation needs repair</span> : null}
      {status === "update_available" ? <span>Update required</span> : null}
      {canOperate ? <button className="primary-button" type="button" onClick={onOperate} disabled={safetyState?.canModifyWowFiles !== true}>{action}</button> : null}
      {status === "check_failed" ? <span className="discovery-error">Update check failed</span> : null}
    </div>
    {canOperate && safetyState?.canModifyWowFiles !== true ? <p className="discovery-error">{safetyState?.isWowRunning ? `Close World of Warcraft (${safetyState.matchingProcessNames.join(", ")}) before ${action.toLowerCase()}.` : "Checking whether World of Warcraft is running before enabling this action."}</p> : null}
  </>;
}

function InstallationOption({ installation }: { installation: WowInstallation }) {
  return <option value={installation.path} disabled={installation.availability !== "available"}>{installation.product?.toUpperCase() ?? "CUSTOM"} — {installation.path}{installation.availability === "unavailable" ? " (unavailable)" : ""}</option>;
}
