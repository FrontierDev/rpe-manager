import type { WowInstallation } from "./configuration";
import type { WowInstallationCandidate } from "./discovery";
import type { SelectedWowInstallationDiscovery } from "./local-discovery";
import type { WowModificationSafetyState } from "./processes";
import type { SelectedProtocolState } from "./protocol-state";

export interface PhaseOneDiagnostics {
  managerVersion: string;
  operatingSystem: string;
  detectedInstallations: WowInstallationCandidate[];
  selectedInstallation: WowInstallation | null;
  selectedInstallationDiscovery: SelectedWowInstallationDiscovery | null;
  wowSafety: WowModificationSafetyState;
  protocolState: SelectedProtocolState | null;
  protocolStateError: string | null;
}

export interface DiagnosticsCommandError {
  code: "configuration" | "discovery";
  message: string;
}
