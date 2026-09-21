import { describe, expect, it } from "vitest";
import { datasetIdFromExport, nextManualRevision } from "./manual-dataset";

const valid = "RPE_DATASET_V1\n{ format = \"rpe-dataset\", version = 1, dataset = { id = \"example.dataset\" } }";

describe("manual dataset import", () => {
  it("reads the stable ID from a data-only dataset envelope", () => {
    expect(datasetIdFromExport(valid)).toBe("example.dataset");
  });

  it("ignores nested IDs and reads the dataset table ID", () => {
    const nested = "RPE_DATASET_V1\n{ dataset = { achievements = { { id = \"criterion_1\" } }, id = \"dataset_1\", format = \"rpe-dataset\", version = 1 } }";
    expect(datasetIdFromExport(nested)).toBe("dataset_1");
  });

  it.each(["", "not a dataset", "RPE_DATASET_V1\n{}", "RPE_DATASET_V1\n{ dataset = { id = \" bad \" } }"])("rejects invalid exports", (payload) => {
    expect(() => datasetIdFromExport(payload)).toThrow();
  });

  it("starts at one and increments a consistent manual manifest revision", () => {
    expect(nextManualRevision(null, "manual.hash")).toBe(1);
    expect(nextManualRevision({ isStale: false, accounts: [{ accountId: "A", availability: "available", pendingOperations: [], operationResults: [], installedPackages: [{ catalogueId: "manual.hash", packageType: "dataset", datasetId: "example.dataset", revision: 4, hash: "a", installedAt: 0 }] }] }, "manual.hash")).toBe(5);
  });

  it("blocks divergent account revisions", () => {
    const state = { isStale: false, accounts: [
      { accountId: "A", availability: "available" as const, pendingOperations: [], operationResults: [], installedPackages: [{ catalogueId: "manual.hash", packageType: "dataset" as const, datasetId: "example.dataset", revision: 1, hash: "a", installedAt: 0 }] },
      { accountId: "B", availability: "available" as const, pendingOperations: [], operationResults: [], installedPackages: [{ catalogueId: "manual.hash", packageType: "dataset" as const, datasetId: "example.dataset", revision: 2, hash: "b", installedAt: 0 }] },
    ] };
    expect(() => nextManualRevision(state, "manual.hash")).toThrow("different installed revisions");
  });
});
