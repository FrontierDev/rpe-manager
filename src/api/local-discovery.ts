import { invoke } from "@tauri-apps/api/core";
import type {
  LocalDiscoveryCommandError,
  SelectedWowInstallationDiscovery,
} from "../models/local-discovery";

export async function discoverSelectedWowInstallation(): Promise<SelectedWowInstallationDiscovery> {
  return invoke<SelectedWowInstallationDiscovery>(
    "discover_selected_wow_installation",
  );
}

export function isLocalDiscoveryCommandError(
  error: unknown,
): error is LocalDiscoveryCommandError {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error
  );
}
