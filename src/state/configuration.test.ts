import { describe, expect, it } from "vitest";
import { createDefaultManagerConfiguration } from "../models/configuration";
import { selectAccounts } from "./configuration";

function configurationWithInstallations() {
  const configuration = createDefaultManagerConfiguration();
  configuration.installations = [
    { id: "retail", product: "retail", path: "C:\\WoW\\_retail_", availability: "available" },
    { id: "ptr", product: "ptr", path: "C:\\WoW\\_ptr_", availability: "available" },
  ];
  return configuration;
}

describe("selectAccounts", () => {
  it("persists an explicit empty selection", () => {
    const configuration = configurationWithInstallations();
    const result = selectAccounts({ configuration, recoveryMessage: null }, "retail", []);

    expect(result.configuration.selectedAccountIds).toEqual({ retail: [] });
  });

  it("keeps selections scoped to their World of Warcraft installation", () => {
    const configuration = configurationWithInstallations();
    const retail = selectAccounts({ configuration, recoveryMessage: null }, "retail", ["ACCOUNT_A"]);
    const result = selectAccounts(retail, "ptr", ["ACCOUNT_B"]);

    expect(result.configuration.selectedAccountIds).toEqual({
      retail: ["ACCOUNT_A"],
      ptr: ["ACCOUNT_B"],
    });
  });
});
