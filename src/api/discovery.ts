import { invoke } from "@tauri-apps/api/core";
import type {
  WowDiscoveryCommandError,
  WowInstallationCandidate,
  WowInstallationSelectionResult,
} from "../models/discovery";

export async function discoverWowInstallations(): Promise<
  WowInstallationCandidate[]
> {
  return invoke<WowInstallationCandidate[]>("discover_wow_installations");
}

export async function selectWowInstallation(
  path: string,
): Promise<WowInstallationSelectionResult> {
  return invoke<WowInstallationSelectionResult>("select_wow_installation", { path });
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

export function discoveryErrorMessage(error: unknown): string {
  if (isWowDiscoveryCommandError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return "The selected folder could not be configured.";
}
