import type { AccountProtocolState } from "./protocol-state";
import type { OperationResult } from "./protocol-state";

export type DatasetStatus =
  | "not_installed"
  | "installed"
  | "update_available"
  | "locally_modified"
  | "unavailable"
  | "pending"
  | "removal_pending"
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

/** RPE confirms this identity has no canonical dataset left to remove. */
export function isAlreadyAbsentRemoval(result: OperationResult): boolean {
  return result.operation === "remove_dataset"
    && result.status === "failed"
    && result.error?.code === "remove_rejected"
    && result.error.detail.toLowerCase().includes("already absent");
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
      if (operation.operation === "remove_dataset") {
        setRow({
          id: `${operation.catalogueId}/${operation.datasetId}`,
          dataset: manualDetails[operation.datasetId]?.name ?? operation.datasetId,
          publisher: operation.catalogueId.startsWith("manual.") ? "Manual" : operation.catalogueId,
          installedRevision: operation.revision,
          availableRevision: null,
          status: "removal_pending",
          category: manualDetails[operation.datasetId]?.category ?? "Unknown",
          catalogueId: operation.catalogueId,
          datasetId: operation.datasetId,
          hash: operation.hash,
        });
        continue;
      }
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
      if (account.operationResults.some((result) => isAlreadyAbsentRemoval(result)
        && result.catalogueId === item.catalogueId
        && result.datasetId === item.datasetId
        && result.revision === item.revision
        && result.hash === item.hash)) continue;
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
      if (result.operation === "remove_dataset") {
        // A successful removal disappears once RPE has advanced its manifest.
        // Retain only a persisted failure so the user can see why it remains.
        if (result.status === "failed" && !isAlreadyAbsentRemoval(result) && result.catalogueId && result.datasetId) {
          setRow({
            id: `${result.catalogueId}/${result.datasetId}`,
            dataset: manualDetails[result.datasetId]?.name ?? result.datasetId,
            publisher: result.catalogueId.startsWith("manual.") ? "Manual" : result.catalogueId,
            installedRevision: result.revision ?? null,
            availableRevision: null,
            status: "failed",
            category: manualDetails[result.datasetId]?.category ?? "Unknown",
            catalogueId: result.catalogueId,
            datasetId: result.datasetId,
            hash: result.hash ?? null,
          });
        }
        continue;
      }
      if (result.operation !== "install_dataset" && result.operation !== "install_ruleset") continue;
      // A terminal success records history, not current installation. The
      // installed manifest is the authoritative source for table membership.
      if (result.status === "succeeded") continue;
      const isRuleset = result.operation === "install_ruleset";
      const catalogueId = result.catalogueId ?? "local";
      const datasetId = result.datasetId ?? "ruleset-import";
      setRow({
        id: isRuleset ? `ruleset/${result.requestId}` : `${catalogueId}/${datasetId}`,
        dataset: isRuleset ? "Ruleset import" : manualDetails[datasetId]?.name ?? datasetId,
        publisher: isRuleset || catalogueId.startsWith("manual.") ? "Manual" : catalogueId,
        installedRevision: result.revision ?? null,
        availableRevision: null,
        status: "failed",
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
    case "pending":
    case "removal_pending": return 3;
    case "installed": return 2;
    default: return 1;
  }
}
