import type { AccountProtocolState, InstalledPackageRecord } from "./protocol-state";

export interface CataloguePackage {
  catalogueId: string;
  publisherId: string;
  publisherType: string;
  publisherDisplayName: string;
  packageType: string;
  nativeRpeId: string;
  datasetType: string | null;
  datasetGroup: string | null;
  name: string;
  currentRevision: number;
  protected: boolean;
}

export type CatalogueInstallState = "not_installed" | "queued" | "installed" | "update_available" | "failed";

export interface CataloguePackagePresentation {
  state: CatalogueInstallState;
  installedPackage: InstalledPackageRecord | null;
  installedRevision: number | null;
  availableRevision: number;
  hasUpdate: boolean;
}

export function catalogueInstallState(pkg: CataloguePackage, accounts: AccountProtocolState[]): CatalogueInstallState {
  const matching = (item: { catalogueId?: string }) => item.catalogueId === pkg.catalogueId;
  if (accounts.some((account) => account.pendingOperations.some(matching))) return "queued";
  if (accounts.some((account) => account.operationResults.some((result) => matching(result) && result.status === "failed"))) return "failed";
  const installed = accounts.flatMap((account) => account.installedPackages).filter((item) => item.catalogueId === pkg.catalogueId);
  if (installed.length === 0) return "not_installed";
  return installed.some((item) => item.revision < pkg.currentRevision) ? "update_available" : "installed";
}

export function installedCatalogueIdentity(pkg: CataloguePackage, accounts: AccountProtocolState[]) {
  return accounts
    .flatMap((account) => account.installedPackages)
    .filter((item) => item.catalogueId === pkg.catalogueId)
    .sort((left, right) => right.revision - left.revision)[0] ?? null;
}

/**
 * The catalogue's manifest is authoritative for catalogue package state and
 * revisions. Local dataset rows only provide Manager-specific metadata such
 * as the removal identity.
 */
export function cataloguePackagePresentation(pkg: CataloguePackage, accounts: AccountProtocolState[]): CataloguePackagePresentation {
  const installed = installedCatalogueIdentity(pkg, accounts);
  return {
    state: catalogueInstallState(pkg, accounts),
    installedPackage: installed,
    installedRevision: installed?.revision ?? null,
    availableRevision: pkg.currentRevision,
    hasUpdate: installed !== null && installed.revision < pkg.currentRevision,
  };
}
