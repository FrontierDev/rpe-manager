import type { ManagerConfiguration } from "../models/configuration";
import type { WowInstallationCandidate } from "../models/discovery";

export type HomeState = "loading" | "first_run" | "error" | "unavailable" | "ready";

export function getHomeState(
  configuration: ManagerConfiguration | null,
  candidates: WowInstallationCandidate[],
  errorMessage: string | null,
): HomeState {
  if (errorMessage !== null) return "error";
  if (configuration === null) return "loading";
  const selectedInstallation = configuration.installations.find(
    (installation) => installation.id === configuration.selectedInstallationId,
  );
  if (selectedInstallation?.availability === "unavailable") return "unavailable";
  if (selectedInstallation !== undefined) return "ready";
  return candidates.some((candidate) => candidate.availability === "available")
    ? "ready"
    : "first_run";
}
