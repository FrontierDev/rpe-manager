export type BackupReason =
  | "before_addon_modification"
  | "before_saved_variables_modification"
  | "manual";

export interface BackupMetadata {
  id: string;
  createdAtUnixMillis: number;
  installationId: string;
  accountId: string | null;
  reason: BackupReason;
  sourceFileName: string;
  byteLength: number;
}

export interface BackupCommandError {
  code: "application_data_path" | "backup";
  message: string;
}
