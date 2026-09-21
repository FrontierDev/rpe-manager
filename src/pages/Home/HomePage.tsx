import { Panel } from "../../../core/ui/Panel";
import { InstallationList } from "../../components/InstallationList";
import { SafetyPanel } from "../../components/SafetyPanel";
import type { ManagerConfiguration } from "../../models/configuration";
import type { WowInstallationCandidate } from "../../models/discovery";
import type { WowInstallationProductChoice } from "../../models/discovery";
import type { WowModificationSafetyState } from "../../models/processes";
import type { HomeState } from "../../app/manager-state";

interface HomePageProps {
  state: HomeState;
  configuration: ManagerConfiguration | null;
  candidates: WowInstallationCandidate[];
  productChoices: WowInstallationProductChoice[] | null;
  safetyState: WowModificationSafetyState | null;
  safetyErrorMessage: string | null;
  errorMessage: string | null;
  isLoading: boolean;
  isSelecting: boolean;
  isSafetyChecking: boolean;
  onRefresh: () => void;
  onRecheckSafety: () => void;
  onChooseFolder: () => void;
  onSelectCandidate: (path: string) => void;
  onSelectProduct: (path: string) => void;
}

export function HomePage(props: HomePageProps) {
  const availableCount = props.candidates.filter((candidate) => candidate.availability === "available").length;
  const selected = props.configuration?.installations.find(
    (installation) => installation.id === props.configuration?.selectedInstallationId,
  );
  const title = props.state === "first_run" ? "Welcome to" : "World of Warcraft";
  const description = props.state === "first_run"
    ? "Locate a World of Warcraft installation to begin managing your local RPEngine setup."
    : selected
      ? `Selected installation: ${selected.path}`
      : "Review detected installations or choose a folder manually.";

  return (
    <>
      <div className="eyebrow"><span className="eyebrow-line" />YOUR LOCAL CONTROL CENTRE</div>
      <section className="welcome-section" aria-labelledby="welcome-heading">
        <div className="welcome-copy"><p className="welcome-kicker">{title}</p><h1 id="welcome-heading">RPEngine <span>Manager</span></h1><p className="welcome-description">{description}</p></div>
        <div className="welcome-orbit" aria-hidden="true"><div className="orbit orbit-outer" /><div className="orbit orbit-inner" /><div className="orbit-core">RP</div></div>
      </section>

      <section className="safety-section"><SafetyPanel state={props.safetyState} errorMessage={props.safetyErrorMessage} isChecking={props.isSafetyChecking} onRecheck={props.onRecheckSafety} /></section>

      <section className="foundation-section" aria-labelledby="discovery-heading">
        <div className="section-heading"><div><p className="section-overline">WORLD OF WARCRAFT</p><h2 id="discovery-heading">Installation discovery</h2></div><button className="secondary-button" type="button" onClick={props.onRefresh} disabled={props.isLoading || props.isSelecting}>Refresh</button></div>
        {props.errorMessage ? <p className="discovery-error">{props.errorMessage}</p> : null}
        <Panel className="discovery-panel"><div><p className="status-label">Automatic discovery</p><h3>{props.isLoading ? "Looking for World of Warcraft..." : availableCount > 0 ? `${availableCount} installation${availableCount === 1 ? "" : "s"} found` : "World of Warcraft was not found automatically"}</h3><p className="status-detail">Discovery reads candidate folders only. It does not change World of Warcraft files.</p></div><button className="primary-button" type="button" onClick={props.onChooseFolder} disabled={props.isSelecting}>{props.isSelecting ? "Checking folder..." : "Select WoW folder"}</button></Panel>
        {props.productChoices !== null ? <Panel className="product-choice-panel" aria-labelledby="product-choice-heading"><div><p className="status-label">WORLD OF WARCRAFT INSTALLATION FOUND</p><h3 id="product-choice-heading">Choose a product</h3><p className="status-detail">The selected folder contains more than one valid World of Warcraft product.</p></div><div className="product-choice-list">{props.productChoices.map((choice) => <button className="product-choice-button" type="button" key={choice.path} onClick={() => props.onSelectProduct(choice.path)} disabled={props.isSelecting}><strong>{choice.product.toUpperCase()}</strong><span>{choice.path}</span></button>)}</div></Panel> : null}
        <InstallationList candidates={props.candidates} selectedInstallationId={props.configuration?.selectedInstallationId ?? null} isSelecting={props.isSelecting} onSelect={props.onSelectCandidate} />
      </section>
    </>
  );
}
