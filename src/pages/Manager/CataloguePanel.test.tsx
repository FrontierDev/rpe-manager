import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CataloguePanel } from "./CataloguePanel";
import type { CataloguePackage } from "../../models/catalogue";
import type { DatasetRow } from "../../models/datasets";
import type { AccountProtocolState, SelectedProtocolState } from "../../models/protocol-state";

const pkg: CataloguePackage = {
  catalogueId: "catalogue.one",
  publisherId: "publisher",
  publisherType: "guild",
  publisherDisplayName: "Publisher",
  packageType: "dataset",
  nativeRpeId: "native.one",
  datasetType: "campaign",
  datasetGroup: null,
  name: "One",
  currentRevision: 4,
  protected: false,
};

const installedRow: DatasetRow = {
  id: "catalogue.one/native.one",
  dataset: "native.one",
  publisher: "catalogue.one",
  installedRevision: 2,
  availableRevision: null,
  status: "installed",
  category: "Unknown",
  catalogueId: "catalogue.one",
  datasetId: "native.one",
  hash: "hash",
};

const account = (overrides: Partial<AccountProtocolState> = {}): AccountProtocolState => ({
  accountId: "ACCOUNT",
  availability: "available",
  pendingOperations: [],
  operationResults: [],
  installedPackages: [],
  ...overrides,
});

function renderPanel(packages: CataloguePackage[], localRows: DatasetRow[], protocol: AccountProtocolState[], fetchState: "loading" | "complete" | "error" = "complete") {
  const state: SelectedProtocolState = { isStale: false, accounts: protocol };
  return renderToStaticMarkup(<CataloguePanel packages={packages} localRows={localRows} fetchState={fetchState} error={fetchState === "error" ? "Catalogue unavailable" : null} protocolState={state} selectedAccounts={["ACCOUNT"]} controls={null} imports={null} onRefresh={() => undefined} onInstall={async () => "queued"} onRemove={async () => "queued"} />);
}

describe("CataloguePanel catalogue reconciliation", () => {
  it("shows an installed withdrawn package with its removal identity and no install action", () => {
    const markup = renderPanel([], [installedRow], [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 2, hash: "hash", installedAt: 1 }] })]);

    expect(markup).toContain("Installed — no longer listed");
    expect(markup).toContain("Catalogue ID: catalogue.one");
    expect(markup).toContain(">Remove<");
    expect(markup).not.toContain(">Install<");
    expect(markup).not.toContain(">Update<");
  });

  it("does not render a withdrawn package when it is not installed", () => {
    const markup = renderPanel([], [], [account()]);

    expect(markup).not.toContain("catalogue.one");
    expect(markup).not.toContain("Installed — no longer listed");
  });

  it("does not infer withdrawal while the catalogue is unavailable", () => {
    const markup = renderPanel([], [installedRow], [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 2, hash: "hash", installedAt: 1 }] })], "error");

    expect(markup).toContain("Catalogue unavailable");
    expect(markup).not.toContain("Installed — no longer listed");
    expect(markup).toContain(">Remove<");
  });

  it("restores normal update comparison when the same catalogue ID returns", () => {
    const markup = renderPanel([pkg], [installedRow], [account({ installedPackages: [{ catalogueId: "catalogue.one", packageType: "dataset", datasetId: "native.one", revision: 2, hash: "hash", installedAt: 1 }] })]);

    expect(markup).toContain("Update Available");
    expect(markup).toContain(">Update<");
    expect(markup).not.toContain("no longer listed");
  });

  it("renders queued catalogue work as Login Required until reconciliation", () => {
    const markup = renderPanel([pkg], [], [account({ pendingOperations: [{ requestId: "request-1", operation: "install_dataset", catalogueId: "catalogue.one", datasetId: "native.one", revision: 4, hash: "hash", hasPayload: true }] })]);

    expect(markup).toContain("Login Required");
    expect(markup).not.toContain(">Queued<");
  });
});
