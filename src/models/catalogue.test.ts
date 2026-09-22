import { describe, expect, it } from "vitest";
import { catalogueInstallState, cataloguePackagePresentation, type CataloguePackage } from "./catalogue";

const pkg: CataloguePackage = { catalogueId: "catalogue.one", publisherId: "p", publisherType: "player", publisherDisplayName: "P", packageType: "dataset", nativeRpeId: "native.one", datasetType: "campaign", datasetGroup: null, name: "One", currentRevision: 2, protected: false };
const account = (overrides = {}) => ({ accountId: "A", availability: "available" as const, pendingOperations: [], operationResults: [], installedPackages: [], ...overrides });

describe("catalogueInstallState", () => {
  it("derives queued, failed, installed, and update states from persisted protocol data", () => {
    expect(catalogueInstallState(pkg, [account({ pendingOperations: [{ requestId: "r", operation: "install_dataset", catalogueId: "catalogue.one", datasetId: "native.one", revision: 2, hash: "a", hasPayload: true }] })])).toBe("queued");
    expect(catalogueInstallState(pkg, [account({ operationResults: [{ requestId: "r", operation: "install_dataset", status: "failed", catalogueId: "catalogue.one" }] })])).toBe("failed");
    expect(catalogueInstallState(pkg, [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 1, hash: "a", installedAt: 1 }] })])).toBe("update_available");
    expect(catalogueInstallState(pkg, [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 2, hash: "a", installedAt: 1 }] })])).toBe("installed");
  });

  it("presents installed and available revisions independently for a current package", () => {
    const presentation = cataloguePackagePresentation(pkg, [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 2, hash: "a", installedAt: 1 }] })]);

    expect(presentation).toMatchObject({ state: "installed", installedRevision: 2, availableRevision: 2, hasUpdate: false });
  });

  it("preserves the installed revision and exposes an update when the catalogue is newer", () => {
    const newerPackage = { ...pkg, currentRevision: 4 };
    const presentation = cataloguePackagePresentation(newerPackage, [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 2, hash: "a", installedAt: 1 }] })]);

    expect(presentation).toMatchObject({ state: "update_available", installedRevision: 2, availableRevision: 4, hasUpdate: true });
  });

  it("shows an installable package with only its available revision when it is absent", () => {
    const presentation = cataloguePackagePresentation(pkg, [account()]);

    expect(presentation).toMatchObject({ state: "not_installed", installedRevision: null, availableRevision: 2, hasUpdate: false });
  });

  it("keeps the installed and available revisions visible while an update is queued or failed", () => {
    const newerPackage = { ...pkg, currentRevision: 4 };
    const installedPackages = [{ catalogueId: "catalogue.one", packageType: "dataset" as const, datasetId: "native.one", revision: 2, hash: "a", installedAt: 1 }];
    const pendingOperations = [{ requestId: "r", operation: "install_dataset" as const, catalogueId: "catalogue.one", datasetId: "native.one", revision: 4, hash: "b", hasPayload: true }];
    const operationResults = [{ requestId: "r", operation: "install_dataset" as const, status: "failed" as const, catalogueId: "catalogue.one", datasetId: "native.one", revision: 4 }];

    expect(cataloguePackagePresentation(newerPackage, [account({ installedPackages, pendingOperations })])).toMatchObject({ state: "queued", installedRevision: 2, availableRevision: 4, hasUpdate: true });
    expect(cataloguePackagePresentation(newerPackage, [account({ installedPackages, operationResults })])).toMatchObject({ state: "failed", installedRevision: 2, availableRevision: 4, hasUpdate: true });
  });
});
