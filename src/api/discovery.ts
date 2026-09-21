import { invoke } from "@tauri-apps/api/core";
import type { ManagerConfiguration } from "../models/configuration";
import type {
  WowDiscoveryCommandError,
  WowInstallationCandidate,
} from "../models/discovery";

export async function discoverWowInstallations(): Promise<
  WowInstallationCandidate[]
> {
  return invoke<WowInstallationCandidate[]>("discover_wow_installations");
}

export async function selectWowInstallation(
  path: string,
): Promise<ManagerConfiguration> {
  return invoke<ManagerConfiguration>("select_wow_installation", { path });
}

export function isWowDiscoveryCommandError(
  error: unknown,
): error is WowDiscoveryCommandError {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error
  );
}
