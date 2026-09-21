import { invoke } from "@tauri-apps/api/core";
import type { WowModificationSafetyState } from "../models/processes";

export async function getWowModificationSafetyState(): Promise<WowModificationSafetyState> {
  return invoke<WowModificationSafetyState>("get_wow_modification_safety_state");
}
