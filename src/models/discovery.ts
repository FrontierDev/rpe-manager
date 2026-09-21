import type {
  InstallationAvailability,
  ManagerConfiguration,
  WowProduct,
} from "./configuration";

export type WowDiscoverySource =
  | "configured"
  | "common_windows_location"
  | "battle_net";

export interface WowInstallationCandidate {
  id: string;
  product: WowProduct | null;
  path: string;
  availability: InstallationAvailability;
  source: WowDiscoverySource;
  unavailableReason: string | null;
}

export interface WowDiscoveryCommandError {
  code: "configuration" | "invalid_installation_path";
  message: string;
}

export interface WowInstallationProductChoice {
  product: WowProduct;
  path: string;
}

export type WowInstallationSelectionResult =
  | { status: "configured"; configuration: ManagerConfiguration }
  | { status: "multiple_products"; products: WowInstallationProductChoice[] };
