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
});
