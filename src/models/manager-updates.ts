export type ManagerUpdateStatus = "idle" | "checking" | "up_to_date" | "update_available" | "downloading" | "installing" | "failed";

export interface ManagerUpdateMetadata {
  currentVersion: string;
  availableVersion: string;
  releaseDate: string | null;
  releaseNotes: string | null;
}

export interface ManagerUpdateState {
  status: ManagerUpdateStatus;
  installedVersion: string | null;
  update: ManagerUpdateMetadata | null;
  downloadedBytes: number | null;
  totalBytes: number | null;
  error: string | null;
}

export const initialManagerUpdateState: ManagerUpdateState = { status: "idle", installedVersion: null, update: null, downloadedBytes: null, totalBytes: null, error: null };
