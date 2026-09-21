import type { WowInstallation } from "./configuration";

export interface WowAccount {
  id: string;
  path: string;
}

export type RPEngineInstallationStatus =
  | "not_installed"
  | "installed"
  | "damaged"
  | "version_unavailable"
  | "unsupported";

export interface RPEngineInstallation {
  status: RPEngineInstallationStatus;
  version: string | null;
  detail: string | null;
}

export interface SelectedWowInstallationDiscovery {
  installation: WowInstallation;
  accounts: WowAccount[];
  rpengine: RPEngineInstallation;
}

export interface LocalDiscoveryCommandError {
  code: "accounts" | "configuration" | "no_selected_installation" | "rpengine";
  message: string;
}
