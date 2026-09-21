import type { AccountProtocolState } from "./protocol-state";

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

export function catalogueInstallState(pkg: CataloguePackage, accounts: AccountProtocolState[]): CatalogueInstallState {
  const matching = (item: { catalogueId?: string }) => item.catalogueId === pkg.catalogueId;
  if (accounts.some((account) => account.pendingOperations.some(matching))) return "queued";
  if (accounts.some((account) => account.operationResults.some((result) => matching(result) && result.status === "failed"))) return "failed";
  const installed = accounts.flatMap((account) => account.installedPackages).filter((item) => item.catalogueId === pkg.catalogueId);
  if (installed.length === 0) return "not_installed";
  return installed.some((item) => item.revision < pkg.currentRevision) ? "update_available" : "installed";
}

export function installedCatalogueIdentity(pkg: CataloguePackage, accounts: AccountProtocolState[]) {
  return accounts.flatMap((account) => account.installedPackages).find((item) => item.catalogueId === pkg.catalogueId) ?? null;
}
