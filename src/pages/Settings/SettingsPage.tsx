import { useEffect, useState } from "react";
import { Panel } from "../../../core/ui/Panel";
import { getPhaseOneDiagnostics, isDiagnosticsCommandError } from "../../api/diagnostics";
import type { ManagerConfiguration } from "../../models/configuration";
import type { PhaseOneDiagnostics } from "../../models/diagnostics";
import type { SelectedWowInstallationDiscovery } from "../../models/local-discovery";

interface SettingsPageProps {
  configuration: ManagerConfiguration | null;
  discovery: SelectedWowInstallationDiscovery | null;
  onToggleAccount: (accountId: string, selected: boolean) => void;
}

export function SettingsPage({ configuration, discovery, onToggleAccount }: SettingsPageProps) {
  const [view, setView] = useState<"installations" | "diagnostics">("installations");
  const [diagnostics, setDiagnostics] = useState<PhaseOneDiagnostics | null>(null);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);

  useEffect(() => { if (view === "diagnostics") void loadDiagnostics(); }, [view]);

  async function loadDiagnostics() {
    try {
      setDiagnosticsError(null);
      setDiagnostics(await getPhaseOneDiagnostics());
    } catch (error) {
      setDiagnosticsError(isDiagnosticsCommandError(error) ? error.message : "Diagnostics could not be collected.");
    }
  }

  if (configuration === null) {
    return <section className="page-section"><h1>Settings</h1><p className="status-detail">Loading Manager configuration...</p></section>;
  }

  const selectedAccounts = discovery ? configuration.selectedAccountIds[discovery.installation.id] ?? [] : [];
  return (
    <section className="page-section">
      <div className="section-heading">
        <div><p className="section-overline">MANAGER CONFIGURATION</p><h1>Settings</h1></div>
        <div className="settings-tabs">
          <button className={view === "installations" ? "nav-button nav-button-active" : "nav-button"} type="button" onClick={() => setView("installations")}>Installations</button>
          <button className={view === "diagnostics" ? "nav-button nav-button-active" : "nav-button"} type="button" onClick={() => setView("diagnostics")}>Diagnostics</button>
        </div>
      </div>
      {view === "installations" ? <>
        <Panel className="detail-panel">
          <p className="status-label">CONFIGURED INSTALLATIONS</p>
          {configuration.installations.map((installation) => <div className="settings-row" key={installation.id}><div><h3>{installation.product?.toUpperCase() ?? "CUSTOM"}</h3><p className="status-detail">{installation.path}</p></div><span className={installation.availability === "available" ? "status-good" : "status-bad"}>{installation.id === configuration.selectedInstallationId ? "Selected · " : ""}{installation.availability}</span></div>)}
        </Panel>
        {discovery ? <Panel className="detail-panel"><p className="status-label">ACCOUNTS FOR SELECTED INSTALLATION</p>{discovery.accounts.length === 0 ? <p className="status-detail">No account directories were found.</p> : discovery.accounts.map((account) => <label className="account-option" key={account.id}><input type="checkbox" checked={selectedAccounts.includes(account.id)} onChange={(event) => onToggleAccount(account.id, event.target.checked)} />{account.id}</label>)}</Panel> : null}
      </> : <DiagnosticsView diagnostics={diagnostics} error={diagnosticsError} onRefresh={() => void loadDiagnostics()} />}
    </section>
  );
}

function DiagnosticsView({ diagnostics, error, onRefresh }: { diagnostics: PhaseOneDiagnostics | null; error: string | null; onRefresh: () => void; }) {
  return (
    <Panel className="detail-panel">
      <div className="section-heading"><div><p className="status-label">MANAGER DIAGNOSTICS</p><h2>Current Manager state</h2></div><button className="secondary-button" type="button" onClick={onRefresh}>Refresh report</button></div>
      {error ? <p className="discovery-error">{error}</p> : null}
      {diagnostics === null ? <p className="status-detail">Collecting diagnostics...</p> : <>
        <dl className="diagnostics-list">
          <dt>Manager version</dt><dd>{diagnostics.managerVersion}</dd>
          <dt>Operating system</dt><dd>{diagnostics.operatingSystem}</dd>
          <dt>Detected installations</dt><dd>{diagnostics.detectedInstallations.length}</dd>
          <dt>Selected WoW path</dt><dd>{diagnostics.selectedInstallation?.path ?? "None"}</dd>
          <dt>Detected accounts</dt><dd>{diagnostics.selectedInstallationDiscovery?.accounts.map((account) => account.id).join(", ") || "None"}</dd>
          <dt>RPEngine</dt><dd>{diagnostics.selectedInstallationDiscovery ? `${diagnostics.selectedInstallationDiscovery.rpengine.status}${diagnostics.selectedInstallationDiscovery.rpengine.version ? ` · ${diagnostics.selectedInstallationDiscovery.rpengine.version}` : ""}` : "Unavailable"}</dd>
          <dt>WoW safety</dt><dd>{diagnostics.wowSafety.isWowRunning ? `Running: ${diagnostics.wowSafety.matchingProcessNames.join(", ")}` : "Not running"}</dd>
          <dt>Protocol snapshot</dt><dd>{diagnostics.protocolState === null ? "Unavailable" : diagnostics.protocolState.isStale ? "Stale on disk while WoW is running" : "Current on-disk snapshot"}</dd>
        </dl>
        {diagnostics.protocolStateError ? <p className="discovery-error">{diagnostics.protocolStateError}</p> : null}
        {diagnostics.protocolState ? <div className="diagnostic-protocol-list">{diagnostics.protocolState.accounts.map((account) => <div className="protocol-result" key={account.accountId}><span className={`protocol-availability protocol-${account.availability}`}>{account.availability.replaceAll("_", " ")}</span><span>{account.accountId}</span><small>{account.protocolVersion === undefined ? account.error?.message ?? "No protocol root" : `v${account.protocolVersion} · ${account.pendingOperations.length} pending · ${account.installedPackages.length} packages`}</small></div>)}</div> : null}
      </>}
    </Panel>
  );
}
