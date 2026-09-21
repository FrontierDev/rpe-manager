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
  category: string;
  catalogueId: string | null;
  datasetId: string | null;
  hash: string | null;
}

export function getDatasetRows(protocolState: AccountProtocolState[], manualDetails: Record<string, { name: string; category: string }> = {}): DatasetRow[] {
  const rows = new Map<string, DatasetRow>();
  const setRow = (row: DatasetRow) => {
    const existing = rows.get(row.id);
    // An installed manifest is authoritative over an old successful result,
    // while pending/failed operations remain useful before installation.
    if (existing === undefined || statusPriority(row.status) >= statusPriority(existing.status)) {
      rows.set(row.id, row);
    }
  };
  for (const account of protocolState) {
    for (const operation of account.pendingOperations) {
      if (operation.operation === "remove_dataset") continue;
      const isRuleset = operation.operation === "install_ruleset";
      setRow({
        id: isRuleset ? `ruleset/${operation.requestId}` : `${operation.catalogueId}/${operation.datasetId}`,
        dataset: isRuleset ? "Ruleset import" : operation.datasetId,
        publisher: isRuleset || operation.catalogueId.startsWith("manual.") ? "Manual" : operation.catalogueId,
        installedRevision: null,
        availableRevision: null,
        status: "pending",
        category: isRuleset ? "Ruleset" : manualDetails[operation.datasetId]?.category ?? "Unknown",
        catalogueId: operation.catalogueId,
        datasetId: operation.datasetId,
        hash: operation.hash,
      });
    }
    for (const item of account.installedPackages) {
      const id = `${item.catalogueId}/${item.datasetId}`;
      const existing = rows.get(id);
      if (existing === undefined || (existing.installedRevision ?? 0) < item.revision) {
        setRow({
          id,
          dataset: manualDetails[item.datasetId]?.name ?? item.datasetId,
          publisher: item.catalogueId.startsWith("manual.") ? "Manual" : item.catalogueId,
          installedRevision: item.revision,
          availableRevision: null,
          status: "installed",
          category: manualDetails[item.datasetId]?.category ?? "Unknown",
          catalogueId: item.catalogueId,
          datasetId: item.datasetId,
          hash: item.hash,
        });
      }
    }
    for (const result of account.operationResults) {
      if (result.operation !== "install_dataset" && result.operation !== "install_ruleset") continue;
      const isRuleset = result.operation === "install_ruleset";
      const catalogueId = result.catalogueId ?? "local";
      const datasetId = result.datasetId ?? "ruleset-import";
      setRow({
        id: isRuleset ? `ruleset/${result.requestId}` : `${catalogueId}/${datasetId}`,
        dataset: isRuleset ? "Ruleset import" : manualDetails[datasetId]?.name ?? datasetId,
        publisher: isRuleset || catalogueId.startsWith("manual.") ? "Manual" : catalogueId,
        installedRevision: result.revision ?? null,
        availableRevision: null,
        status: result.status === "failed" ? "failed" : "installed",
        category: isRuleset ? "Ruleset" : manualDetails[datasetId]?.category ?? "Unknown",
        catalogueId,
        datasetId,
        hash: result.hash ?? null,
      });
    }
  }
  return [...rows.values()].sort((left, right) => left.dataset.localeCompare(right.dataset));
}

function statusPriority(status: DatasetStatus): number {
  switch (status) {
    case "failed": return 4;
    case "pending": return 3;
    case "installed": return 2;
    default: return 1;
  }
}
