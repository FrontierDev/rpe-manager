import { describe, expect, it } from "vitest";
import { discoveryErrorMessage } from "./discovery";

describe("discoveryErrorMessage", () => {
  it("keeps the validation message returned by the backend", () => {
    expect(discoveryErrorMessage({
      code: "invalid_installation_path",
      message: "Expected Wow.exe or Wow-64.exe in C:\\Games\\World of Warcraft\\_retail_",
    })).toBe("Expected Wow.exe or Wow-64.exe in C:\\Games\\World of Warcraft\\_retail_");
  });
});
