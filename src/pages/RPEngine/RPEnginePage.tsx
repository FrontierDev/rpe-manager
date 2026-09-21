import { Panel } from "../../../core/ui/Panel";
import type { SelectedWowInstallationDiscovery } from "../../models/local-discovery";

interface RPEnginePageProps { discovery: SelectedWowInstallationDiscovery | null; }

export function RPEnginePage({ discovery }: RPEnginePageProps) {
  if (discovery === null) return <section className="page-section"><h1>RPEngine</h1><Panel className="empty-panel"><h2>Select a World of Warcraft installation</h2><p className="status-detail">RPEngine status becomes available after an installation is configured.</p></Panel></section>;
  const { rpengine, accounts, installation } = discovery;
  return <section className="page-section"><div className="section-heading"><div><p className="section-overline">SELECTED INSTALLATION</p><h1>RPEngine</h1></div><span className="phase-badge">READ ONLY</span></div><Panel className="detail-panel"><p className="status-label">INSTALLATION</p><h2>{installation.path}</h2><p className="status-detail">RPEngine is {rpengine.status.replaceAll("_", " ")}{rpengine.version ? ` · Version ${rpengine.version}` : ""}.</p>{rpengine.detail ? <p className="discovery-error">{rpengine.detail}</p> : null}</Panel><div className="inspection-grid"><Panel className="inspection-card"><p className="status-label">RPEngine status</p><h3>{rpengine.status.replaceAll("_", " ")}</h3><p className="status-detail">{rpengine.version ?? "No readable version is available."}</p></Panel><Panel className="inspection-card"><p className="status-label">Accounts</p><h3>{accounts.length} account{accounts.length === 1 ? "" : "s"} detected</h3><p className="status-detail">{accounts.length === 0 ? "No account directories were found." : accounts.map((account) => account.id).join(", ")}</p></Panel></div></section>;
}
