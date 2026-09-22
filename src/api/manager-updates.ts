import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { getVersion } from "@tauri-apps/api/app";

export interface ManagerUpdateHandle {
  currentVersion: string;
  version: string;
  date?: string;
  body?: string;
  downloadAndInstall(onEvent?: (event: DownloadEvent) => void): Promise<void>;
  close(): Promise<void>;
}

export interface ManagerUpdaterApi { getCurrentVersion(): Promise<string>; check(): Promise<ManagerUpdateHandle | null>; }

export const tauriManagerUpdater: ManagerUpdaterApi = { getCurrentVersion: getVersion, check: async () => check() as Promise<Update | null> };
