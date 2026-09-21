import { describe, expect, it } from "vitest";
import { catalogueErrorMessage } from "./catalogue";

describe("catalogueErrorMessage", () => {
  it("preserves Tauri structured command errors", () => {
    expect(catalogueErrorMessage({ code: "package", message: "invalid_password: the package password was rejected" }, "fallback")).toBe("invalid_password: the package password was rejected");
  });
});
