import { invoke } from "@tauri-apps/api/core";
import type { BackupMetadata } from "../models/backups";

/** Lists Manager-owned backup metadata without exposing backup file paths. */
export async function listManagerBackups(): Promise<BackupMetadata[]> {
  return invoke<BackupMetadata[]>("list_manager_backups");
}
