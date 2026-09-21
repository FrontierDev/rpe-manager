import type { WowInstallation } from "./configuration";
import type { WowInstallationCandidate } from "./discovery";
import type { SelectedWowInstallationDiscovery } from "./local-discovery";
import type { WowModificationSafetyState } from "./processes";

export interface PhaseOneDiagnostics {
  managerVersion: string;
  operatingSystem: string;
  detectedInstallations: WowInstallationCandidate[];
  selectedInstallation: WowInstallation | null;
  selectedInstallationDiscovery: SelectedWowInstallationDiscovery | null;
  wowSafety: WowModificationSafetyState;
}

export interface DiagnosticsCommandError {
  code: "configuration" | "discovery";
  message: string;
}
