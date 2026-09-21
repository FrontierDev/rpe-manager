import { describe, expect, it } from "vitest";
import { createDefaultManagerConfiguration } from "../models/configuration";
import { getHomeState } from "./manager-state";

describe("getHomeState", () => {
  it("shows first run when no installation is configured or found", () => {
    expect(getHomeState(createDefaultManagerConfiguration(), [], null)).toBe("first_run");
  });

  it("keeps discovery failures visible", () => {
    expect(getHomeState(createDefaultManagerConfiguration(), [], "Access denied")).toBe("error");
  });

  it("does not replace an unavailable configured installation", () => {
    const configuration = createDefaultManagerConfiguration();
    configuration.installations = [{
      id: "retail",
      product: "retail",
      path: "C:\\Missing\\_retail_",
      availability: "unavailable",
    }];
    configuration.selectedInstallationId = "retail";

    expect(getHomeState(configuration, [], null)).toBe("unavailable");
  });
});
