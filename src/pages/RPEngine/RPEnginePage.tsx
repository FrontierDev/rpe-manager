import { Panel } from "../../../core/ui/Panel";
import type { SelectedWowInstallationDiscovery } from "../../models/local-discovery";
import type { RpeUpdateState } from "../../models/rpengine-update";
import type { WowModificationSafetyState } from "../../models/processes";
import type {
  AccountProtocolState,
  SelectedProtocolState,
} from "../../models/protocol-state";

interface RPEnginePageProps {
  discovery: SelectedWowInstallationDiscovery | null;
  updateState: RpeUpdateState | null;
  safetyState: WowModificationSafetyState | null;
  isOperating: boolean;
  operationError: string | null;
  onOperate: () => void;
  protocolState: SelectedProtocolState | null;
  protocolErrorMessage: string | null;
  onRefreshProtocol: () => void;
}

export function RPEnginePage({
  discovery,
  updateState,
  safetyState,
  isOperating,
  operationError,
  onOperate,
  protocolState,
  protocolErrorMessage,
  onRefreshProtocol,
}: RPEnginePageProps) {
  if (discovery === null) {
    return <section className="page-section"><Panel className="empty-panel"><h2>Protocol state</h2><p className="status-detail">Select a World of Warcraft installation to inspect local protocol state.</p></Panel></section>;
  }

  const { rpengine, accounts, installation } = discovery;
  return (
    <section className="page-section">
      <AddonManagement state={updateState} safety={safetyState} operating={isOperating} error={operationError} onOperate={onOperate} />
      <div className="section-heading">
        <div><h2>Protocol state</h2></div>
      </div>
      <Panel className="detail-panel">
        <p className="status-label">INSTALLATION</p><h2>{installation.path}</h2>
        <p className="status-detail">RPEngine is {rpengine.status.replaceAll("_", " ")}{rpengine.version ? ` · Version ${rpengine.version}` : ""}.</p>
        {rpengine.detail ? <p className="discovery-error">{rpengine.detail}</p> : null}
      </Panel>
      <div className="inspection-grid">
        <Panel className="inspection-card"><p className="status-label">RPEngine status</p><h3>{rpengine.status.replaceAll("_", " ")}</h3><p className="status-detail">{rpengine.version ?? "No readable version is available."}</p></Panel>
        <Panel className="inspection-card"><p className="status-label">Accounts</p><h3>{accounts.length} account{accounts.length === 1 ? "" : "s"} detected</h3><p className="status-detail">{accounts.length === 0 ? "No account directories were found." : accounts.map((account) => account.id).join(", ")}</p></Panel>
      </div>
      <ProtocolStatusSurface state={protocolState} error={protocolErrorMessage} onRefresh={onRefreshProtocol} />
    </section>
  );
}

function AddonManagement({ state, safety, operating, error, onOperate }: { state: RpeUpdateState | null; safety: WowModificationSafetyState | null; operating: boolean; error: string | null; onOperate: () => void }) {
  const action = state?.status === "not_installed" ? "Install RPEngine" : state?.status === "damaged" ? "Repair RPEngine" : "Update RPEngine";
  const actionable = state?.status === "not_installed" || state?.status === "damaged" || state?.status === "update_available";
  return <Panel className="detail-panel"><p className="status-label">RPE ADDON</p><h2>{state?.local.version ? `Installed ${state.local.version}` : "RPEngine"}</h2>
    {state?.latestVersion ? <p className="status-detail">Latest release: {state.latestVersion}</p> : null}
    <p className="status-detail">{state?.status === "current" ? "Up to date" : state?.status === "update_available" ? "Update required" : state?.status === "check_failed" ? "Update checking failed; local installation state is retained." : state?.local.detail ?? state?.status.replaceAll("_", " ") ?? "Loading status"}</p>
    {state?.status === "check_failed" ? <button className="secondary-button" type="button" onClick={() => window.location.reload()}>Retry update check</button> : null}
    {actionable ? <button className="primary-button" type="button" onClick={onOperate} disabled={operating || safety?.canModifyWowFiles !== true}>{operating ? "Working…" : action}</button> : null}
    {safety?.canModifyWowFiles === false ? <p className="discovery-error">Close World of Warcraft ({safety.matchingProcessNames.join(", ")}) before modifying RPEngine.</p> : null}
    {error ? <p className="discovery-error">{error}</p> : null}
  </Panel>;
}

function ProtocolStatusSurface({ state, error, onRefresh }: {
  state: SelectedProtocolState | null;
  error: string | null;
  onRefresh: () => void;
}) {
  return (
    <section className="local-inspection" aria-labelledby="protocol-heading">
      <div className="section-heading">
        <div><h2 id="protocol-heading">External-manager protocol</h2></div>
        <button className="secondary-button" type="button" onClick={onRefresh}>Refresh persisted state</button>
      </div>
      {error ? <p className="discovery-error">{error}</p> : null}
      {state?.isStale ? <p className="protocol-stale">{state.staleReason}</p> : null}
      {state === null ? <Panel className="empty-panel"><p className="status-detail">Select one or more accounts in Settings to inspect their persisted RPEngine protocol state.</p></Panel> : state.accounts.length === 0 ? <Panel className="empty-panel"><p className="status-detail">No accounts are selected for protocol inspection.</p></Panel> : <div className="protocol-account-list">{state.accounts.map((account) => <ProtocolAccount key={account.accountId} account={account} isStale={state.isStale} />)}</div>}
    </section>
  );
}

function ProtocolAccount({ account, isStale }: { account: AccountProtocolState; isStale: boolean }) {
  const terminalResults = account.operationResults;
  return (
    <Panel className="protocol-account">
      <div className="protocol-account-header"><div><p className="status-label">ACCOUNT</p><h3>{account.accountId}</h3></div><span className={`protocol-availability protocol-${account.availability}`}>{account.availability.replaceAll("_", " ")}</span></div>
      {account.error ? <p className="discovery-error">{account.error.message}</p> : null}
      {account.availability === "absent" ? <p className="status-detail">RPEngineManagerDB has not been saved for this account. No package state is inferred.</p> : null}
      {account.availability === "available" ? <>
        <div className="protocol-metrics"><div><span>Protocol</span><strong>v{account.protocolVersion}</strong></div><div><span>Pending</span><strong>{account.pendingOperations.length}</strong></div><div><span>Managed packages</span><strong>{account.installedPackages.length}</strong></div></div>
        {isStale ? <p className="status-detail">The counts and results below are on disk only; they do not confirm the addon's current in-game state.</p> : null}
        <div className="protocol-results"><p className="status-label">PERSISTED TERMINAL RESULTS</p>{terminalResults.length === 0 ? <p className="status-detail">No terminal operation results are persisted.</p> : terminalResults.map((result) => <div className="protocol-result" key={result.requestId}><span className={result.status === "succeeded" ? "status-good" : "status-bad"}>{result.status}</span><span>{result.requestId}</span>{result.error ? <small>{result.error.code}</small> : null}</div>)}</div>
      </> : null}
    </Panel>
  );
}
