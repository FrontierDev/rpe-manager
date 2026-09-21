import { invoke } from "@tauri-apps/api/core";
import type { RpeUpdateState } from "../models/rpengine-update";

export function getRpeUpdateState(): Promise<RpeUpdateState> { return invoke("get_rpengine_update_state"); }
export function installLatestRpe(): Promise<{ version: string }> { return invoke("install_latest_rpengine"); }

export function rpeOperationErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") return error.message;
  return error instanceof Error ? error.message : "RPEngine operation failed.";
}
