import { invoke } from "@tauri-apps/api/core";
import type {
  DiagnosticsCommandError,
  PhaseOneDiagnostics,
} from "../models/diagnostics";

export async function getPhaseOneDiagnostics(): Promise<PhaseOneDiagnostics> {
  return invoke<PhaseOneDiagnostics>("get_phase_one_diagnostics");
}

export function isDiagnosticsCommandError(
  error: unknown,
): error is DiagnosticsCommandError {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error
  );
}
