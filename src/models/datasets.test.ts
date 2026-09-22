import { describe, expect, it } from "vitest";
import { getDatasetRows } from "./datasets";

const account = {
  accountId: "ACCOUNT",
  availability: "available" as const,
  installedPackages: [],
};

describe("dataset table rows", () => {
  it("shows pending dataset and ruleset imports", () => {
    const rows = getDatasetRows([{
      ...account,
      operationResults: [],
      pendingOperations: [
        { requestId: "dataset-1", operation: "install_dataset", catalogueId: "manual.abc", datasetId: "example", revision: 1, hash: "a", hasPayload: true },
        { requestId: "ruleset-1", operation: "install_ruleset", catalogueId: "local", datasetId: "ruleset-import", revision: 1, hash: "b", hasPayload: true },
      ],
    }]);

    expect(rows).toEqual(expect.arrayContaining([
      expect.objectContaining({ dataset: "example", publisher: "Manual", status: "pending" }),
      expect.objectContaining({ dataset: "Ruleset import", publisher: "Manual", status: "pending" }),
    ]));
  });

  it("surfaces a failed import result", () => {
    const rows = getDatasetRows([{
      ...account,
      pendingOperations: [],
      operationResults: [{ requestId: "dataset-1", operation: "install_dataset", status: "failed" as const, catalogueId: "manual.abc", datasetId: "example", revision: 1, hash: "a", error: { code: "import_rejected", detail: "Invalid export" } }],
    }]);

    expect(rows[0]).toMatchObject({ dataset: "example", status: "failed" });
  });

  it("keeps an installed dataset visible as removal pending until RPE processes it", () => {
    const rows = getDatasetRows([{
      ...account,
      pendingOperations: [{ requestId: "remove-1", operation: "remove_dataset", catalogueId: "manual.abc", datasetId: "example", revision: 1, hash: "a", hasPayload: false }],
      operationResults: [],
      installedPackages: [{ catalogueId: "manual.abc", packageType: "dataset", datasetId: "example", revision: 1, hash: "a", installedAt: 1 }],
    }]);

    expect(rows).toEqual([expect.objectContaining({ dataset: "example", status: "removal_pending" })]);
  });

  it("stops tracking a manifest entry when RPE confirms its dataset is already absent", () => {
    const rows = getDatasetRows([{
      ...account,
      pendingOperations: [],
      operationResults: [{ requestId: "remove-1", operation: "remove_dataset", status: "failed" as const, catalogueId: "manual.abc", datasetId: "example", revision: 1, hash: "a", error: { code: "remove_rejected", detail: "The installed dataset is already absent, so canonical removal cannot succeed." } }],
      installedPackages: [{ catalogueId: "manual.abc", packageType: "dataset", datasetId: "example", revision: 1, hash: "a", installedAt: 1 }],
    }]);

    expect(rows).toEqual([]);
  });

  it("does not treat a historical successful install result as an installed manifest entry", () => {
    const rows = getDatasetRows([{
      ...account,
      pendingOperations: [],
      installedPackages: [],
      operationResults: [{ requestId: "install-1", operation: "install_dataset", status: "succeeded" as const, catalogueId: "manual.abc", datasetId: "example", revision: 1, hash: "a" }],
    }]);

    expect(rows).toEqual([]);
  });
});
