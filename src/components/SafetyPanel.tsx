import { Panel } from "../../core/ui/Panel";
import type { WowModificationSafetyState } from "../models/processes";
import { getModificationActionState } from "../state/modification-safety";

interface SafetyPanelProps {
  state: WowModificationSafetyState | null;
  errorMessage: string | null;
  isChecking: boolean;
  onRecheck: () => void;
}

export function SafetyPanel({ state, errorMessage, isChecking, onRecheck }: SafetyPanelProps) {
  const action = getModificationActionState(state);
  const heading = state === null
    ? "Checking World of Warcraft running state..."
    : state.isWowRunning
      ? "World of Warcraft is currently running."
      : "World of Warcraft is not running.";
  const detail = errorMessage ?? (state === null
    ? "Manager-controlled file changes stay unavailable until this check completes."
    : action.disabled
      ? `${action.reason} Changes to SavedVariables while the game is running may be overwritten.`
      : "A fresh process check will be required before Manager-controlled changes can write to AddOns or WTF.");

  return (
    <Panel className={`safety-panel ${state?.isWowRunning ? "safety-running" : "safety-clear"}`}>
      <div><p className="status-label">WRITE SAFETY</p><h2>{heading}</h2><p className="status-detail">{detail}</p></div>
      <button className="secondary-button" type="button" onClick={onRecheck} disabled={isChecking}>
        {isChecking ? "Checking..." : "Recheck"}
      </button>
    </Panel>
  );
}
