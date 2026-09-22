import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ManagerUpdateState } from "../models/manager-updates";
import { ManagerUpdateBannerContent, ManagerUpdateSectionContent, latestLabel } from "./ManagerUpdateStatus";

const callbacks = { onCheck: () => undefined, onInstall: () => undefined };
const baseState: ManagerUpdateState = { status: "idle", installedVersion: "0.1.0", update: null, downloadedBytes: null, totalBytes: null, error: null };

describe("ManagerUpdateStatus", () => {
  it("shows an actionable persistent notification only when an update is available", () => {
    const available = { ...baseState, status: "update_available" as const, update: { currentVersion: "0.1.0", availableVersion: "0.1.1", releaseDate: null, releaseNotes: null } };
    expect(renderToStaticMarkup(<ManagerUpdateBannerContent state={available} {...callbacks} />)).toContain("Update Manager");
    expect(renderToStaticMarkup(<ManagerUpdateBannerContent state={available} {...callbacks} />)).toContain("Installed: 0.1.0");
    expect(renderToStaticMarkup(<ManagerUpdateBannerContent state={{ ...baseState, status: "up_to_date" }} {...callbacks} />)).toBe("");
  });

  it("renders Advanced update status, available version, and release notes", () => {
    const state = { ...baseState, status: "update_available" as const, update: { currentVersion: "0.1.0", availableVersion: "0.1.1", releaseDate: "2026-09-22", releaseNotes: "Security fixes" } };
    const markup = renderToStaticMarkup(<ManagerUpdateSectionContent state={state} {...callbacks} />);
    expect(markup).toContain("Installed");
    expect(markup).toContain("Update to 0.1.1");
    expect(markup).toContain("Security fixes");
  });

  it("reports determinate and indeterminate download progress without allowing duplicate actions", () => {
    const determinate = { ...baseState, status: "downloading" as const, update: { currentVersion: "0.1.0", availableVersion: "0.1.1", releaseDate: null, releaseNotes: null }, downloadedBytes: 512, totalBytes: 1024 };
    expect(renderToStaticMarkup(<ManagerUpdateSectionContent state={determinate} {...callbacks} />)).toContain("Downloading update: 50%");
    const indeterminate = { ...determinate, totalBytes: null };
    expect(renderToStaticMarkup(<ManagerUpdateSectionContent state={indeterminate} {...callbacks} />)).toContain("Downloading update...");
  });

  it("keeps failed checks retryable and labels a successful check as up to date", () => {
    const failed = { ...baseState, status: "failed" as const, error: "signature rejected" };
    expect(renderToStaticMarkup(<ManagerUpdateSectionContent state={failed} {...callbacks} />)).toContain("Retry check");
    expect(latestLabel({ ...baseState, status: "up_to_date" })).toBe("0.1.0 (up to date)");
  });
});
