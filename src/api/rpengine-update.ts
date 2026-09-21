import { invoke } from "@tauri-apps/api/core";
import type { RpeUpdateState } from "../models/rpengine-update";

export function getRpeUpdateState(): Promise<RpeUpdateState> { return invoke("get_rpengine_update_state"); }
export function installLatestRpe(): Promise<{ version: string }> { return invoke("install_latest_rpengine"); }
