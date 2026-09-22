import { invoke } from "@tauri-apps/api/core";

export interface QueueDatasetRequest {
  requestId: string;
  catalogueId: string;
  datasetId: string;
  revision: number;
  hash: string;
  payload: string;
}

export interface QueueDatasetReport {
  accounts: Array<{ accountId: string; status: "queued" | "reconciled" | "failed"; error?: { code: string; message: string } }>;
}

export function queueDataset(request: QueueDatasetRequest): Promise<QueueDatasetReport> {
  return invoke<QueueDatasetReport>("queue_install_dataset", { request });
}

export interface QueueRemoveDatasetRequest {
  requestId: string;
  catalogueId: string;
  datasetId: string;
  revision: number;
  hash: string;
}

export function queueDatasetRemoval(request: QueueRemoveDatasetRequest): Promise<QueueDatasetReport> {
  return invoke<QueueDatasetReport>("queue_remove_dataset", { request });
}
