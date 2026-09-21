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
  if (error instanceof Error) return error.message;
  if (typeof error === "string") {
    const trimmed = error.trim();
    if (trimmed.startsWith("{")) {
      try {
        const parsed: unknown = JSON.parse(trimmed);
        if (isWowDiscoveryCommandError(parsed)) return parsed.message;
        if (typeof parsed === "object" && parsed !== null && "message" in parsed && typeof parsed.message === "string") {
          return parsed.message;
        }
      } catch {
        // The invoke bridge can reject with plain error text rather than JSON.
      }
    }
    if (trimmed.length > 0) return trimmed;
  }
  if (isWowDiscoveryCommandError(error)) return error.message;
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") {
    return error.message;
  }
  return "The selected folder could not be configured.";
}
