import { Panel } from "../../core/ui/Panel";
import type { ManagerUpdateState } from "../models/manager-updates";
import { useManagerUpdates } from "../state/manager-updates";

interface ManagerUpdatePresentationProps {
  state: ManagerUpdateState;
  onCheck: () => void;
  onInstall: () => void;
}

export function ManagerUpdateBanner() {
  const updates = useManagerUpdates();
  return <ManagerUpdateBannerContent state={updates.state} onCheck={() => void updates.checkForManagerUpdate()} onInstall={() => void updates.installManagerUpdate()} />;
}

export function ManagerUpdateBannerContent({ state, onInstall }: ManagerUpdatePresentationProps) {
  if (!isUpdateInProgressOrAvailable(state)) return null;
  const availableVersion = state.update?.availableVersion ?? "the latest version";
  return <aside className="manager-update-banner" aria-live="polite"><div><strong>RPEngine Manager {availableVersion} is available.</strong><span>Installed: {state.installedVersion ?? "unknown"}</span>{state.status === "downloading" ? <DownloadProgress state={state} /> : null}{state.status === "installing" ? <span>Installing - Manager may close while Windows applies the update.</span> : null}</div><button className="primary-button" type="button" disabled={state.status !== "update_available"} onClick={onInstall}>{state.status === "update_available" ? "Update Manager" : state.status === "downloading" ? "Downloading..." : "Installing..."}</button></aside>;
}

export function ManagerUpdateSection() {
  const updates = useManagerUpdates();
  return <ManagerUpdateSectionContent state={updates.state} onCheck={() => void updates.checkForManagerUpdate()} onInstall={() => void updates.installManagerUpdate()} />;
}

export function ManagerUpdateSectionContent({ state, onCheck, onInstall }: ManagerUpdatePresentationProps) {
  const active = isActive(state);
  const availableVersion = state.update?.availableVersion;
  return <Panel className="manager-update-section"><div className="manager-update-heading"><div><p className="status-label">MANAGER UPDATE</p><h2>RPEngine Manager</h2></div><button className="secondary-button" type="button" disabled={active} onClick={onCheck}>{state.status === "checking" ? "Checking..." : "Check for updates"}</button></div><dl className="manager-update-details"><dt>Installed</dt><dd>{state.installedVersion ?? "Checking version..."}</dd><dt>Latest</dt><dd>{latestLabel(state)}</dd></dl>{state.status === "update_available" ? <p className="manager-update-message">Version {availableVersion} is available.</p> : null}{state.status === "downloading" ? <DownloadProgress state={state} /> : null}{state.status === "installing" ? <p className="manager-update-message">Installing - Manager may close while Windows applies the update.</p> : null}{state.status === "failed" ? <p className="discovery-error">{state.error}</p> : null}{state.update?.releaseNotes ? <details className="manager-update-notes"><summary>Release notes</summary><p>{state.update.releaseNotes}</p></details> : null}<div className="manager-update-actions">{availableVersion ? <button className="primary-button" type="button" disabled={active || state.status !== "update_available"} onClick={onInstall}>{state.status === "update_available" ? `Update to ${availableVersion}` : state.status === "downloading" ? "Downloading..." : state.status === "installing" ? "Installing..." : `Update to ${availableVersion}`}</button> : null}{state.status === "failed" ? <button className="secondary-button" type="button" onClick={onCheck}>Retry check</button> : null}</div></Panel>;
}

export function latestLabel(state: ManagerUpdateState): string {
  if (state.status === "checking") return "Checking for updates...";
  if (state.status === "up_to_date") return state.installedVersion ? `${state.installedVersion} (up to date)` : "Up to date";
  if (state.update !== null) return state.update.availableVersion;
  if (state.status === "failed") return "Update check failed";
  return "Not checked";
}

function isActive(state: ManagerUpdateState) { return state.status === "checking" || state.status === "downloading" || state.status === "installing"; }
function isUpdateInProgressOrAvailable(state: ManagerUpdateState) { return state.status === "update_available" || state.status === "downloading" || state.status === "installing"; }
function DownloadProgress({ state }: { state: ManagerUpdateState }) { const { downloadedBytes, totalBytes } = state; if (totalBytes === null || downloadedBytes === null || totalBytes <= 0) return <span className="manager-update-progress">Downloading update...</span>; const percentage = Math.min(100, Math.round(downloadedBytes / totalBytes * 100)); return <span className="manager-update-progress">Downloading update: {percentage}% ({formatBytes(downloadedBytes)} of {formatBytes(totalBytes)})</span>; }
function formatBytes(bytes: number) { return bytes < 1024 * 1024 ? `${Math.round(bytes / 1024)} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`; }
