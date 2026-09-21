import type { AccountProtocolState } from "./protocol-state";

export type DatasetStatus =
  | "not_installed"
  | "installed"
  | "update_available"
  | "locally_modified"
  | "unavailable"
  | "pending"
  | "failed";

/**
 * The catalogue-facing representation used by the Manager table. Catalogue
 * integration can supply availableRevision and publisher without changing UI.
 */
export interface DatasetRow {
  id: string;
  dataset: string;
  publisher: string;
  installedRevision: number | null;
  availableRevision: number | null;
  status: DatasetStatus;
}

export function getDatasetRows(protocolState: AccountProtocolState[]): DatasetRow[] {
  const rows = new Map<string, DatasetRow>();
  for (const account of protocolState) {
    for (const item of account.installedPackages) {
      const id = `${item.catalogueId}/${item.datasetId}`;
      const existing = rows.get(id);
      if (existing === undefined || (existing.installedRevision ?? 0) < item.revision) {
        rows.set(id, {
          id,
          dataset: item.datasetId,
          publisher: item.catalogueId,
          installedRevision: item.revision,
          availableRevision: null,
          status: "installed",
        });
      }
    }
  }
  return [...rows.values()].sort((left, right) => left.dataset.localeCompare(right.dataset));
}
