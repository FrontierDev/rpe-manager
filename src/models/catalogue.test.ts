import { describe, expect, it } from "vitest";
import { catalogueInstallState, type CataloguePackage } from "./catalogue";

const pkg: CataloguePackage = { catalogueId: "catalogue.one", publisherId: "p", publisherType: "player", publisherDisplayName: "P", packageType: "dataset", nativeRpeId: "native.one", datasetType: "campaign", datasetGroup: null, name: "One", currentRevision: 2, protected: false };
const account = (overrides = {}) => ({ accountId: "A", availability: "available" as const, pendingOperations: [], operationResults: [], installedPackages: [], ...overrides });

describe("catalogueInstallState", () => {
  it("derives queued, failed, installed, and update states from persisted protocol data", () => {
    expect(catalogueInstallState(pkg, [account({ pendingOperations: [{ requestId: "r", operation: "install_dataset", catalogueId: "catalogue.one", datasetId: "native.one", revision: 2, hash: "a", hasPayload: true }] })])).toBe("queued");
    expect(catalogueInstallState(pkg, [account({ operationResults: [{ requestId: "r", operation: "install_dataset", status: "failed", catalogueId: "catalogue.one" }] })])).toBe("failed");
    expect(catalogueInstallState(pkg, [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 1, hash: "a", installedAt: 1 }] })])).toBe("update_available");
    expect(catalogueInstallState(pkg, [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 2, hash: "a", installedAt: 1 }] })])).toBe("installed");
  });
});
