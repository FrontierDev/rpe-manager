import type { RPEngineInstallation } from "./local-discovery";

export type RpeUpdateStatus = "not_installed" | "current" | "update_available" | "damaged" | "version_unavailable" | "unsupported" | "check_failed";
export interface RpeUpdateState { local: RPEngineInstallation; status: RpeUpdateStatus; latestVersion: string | null; detail: string | null; }
