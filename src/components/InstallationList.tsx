import { Panel } from "../../core/ui/Panel";
import type { WowInstallationCandidate } from "../models/discovery";

interface InstallationListProps {
  candidates: WowInstallationCandidate[];
  selectedInstallationId: string | null;
  isSelecting: boolean;
  onSelect: (path: string) => void;
}

function productLabel(product: WowInstallationCandidate["product"]) {
  return product === null ? "Custom installation" : product.toUpperCase();
}

export function InstallationList({ candidates, selectedInstallationId, isSelecting, onSelect }: InstallationListProps) {
  return (
    <div className="installation-list" aria-live="polite">
      {candidates.map((candidate) => (
        <Panel className={`installation-card installation-${candidate.availability}`} key={candidate.id}>
          <div className={`status-indicator status-${candidate.availability}`} />
          <p className="status-label">{productLabel(candidate.product)}</p>
          <h3>{candidate.path}</h3>
          <p className="status-detail">
            {candidate.availability === "unavailable" ? candidate.unavailableReason :
              candidate.source === "configured" ? "Configured installation" :
                candidate.source === "battle_net" ? "Found from Battle.net installation information" :
                  "Found in a common Windows location"}
          </p>
          {candidate.availability === "available" ? (
            <button className="link-button" type="button" onClick={() => onSelect(candidate.path)} disabled={isSelecting}>
              {selectedInstallationId === candidate.id ? "Selected" : "Use this installation"}
            </button>
          ) : null}
        </Panel>
      ))}
    </div>
  );
}
