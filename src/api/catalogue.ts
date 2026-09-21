import { invoke } from "@tauri-apps/api/core";
import type { CataloguePackage } from "../models/catalogue";
import type { QueueDatasetReport } from "./datasets";

export function getCataloguePackages(): Promise<{ packages: CataloguePackage[] }> {
  return invoke("get_catalogue_packages", { limit: 100, offset: 0 });
}

export function queueCataloguePackageInstall(request: { catalogueId: string; revision?: number; password?: string }): Promise<QueueDatasetReport> {
  return invoke<QueueDatasetReport>("queue_catalogue_package_install", { request });
}

/** Tauri can reject commands with either a structured error or JSON text. */
export function catalogueErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") return error.message;
  if (typeof error === "string") {
    try {
      const parsed: unknown = JSON.parse(error);
      if (typeof parsed === "object" && parsed !== null && "message" in parsed && typeof parsed.message === "string") return parsed.message;
    } catch { /* Plain error text is also valid. */ }
    if (error.trim()) return error;
  }
  return fallback;
}
