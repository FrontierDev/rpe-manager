import { createContext, createElement, useContext, useEffect, useMemo, useRef, useState, type PropsWithChildren } from "react";
import type { DownloadEvent } from "@tauri-apps/plugin-updater";
import { tauriManagerUpdater, type ManagerUpdateHandle, type ManagerUpdaterApi } from "../api/manager-updates";
import { initialManagerUpdateState, type ManagerUpdateState } from "../models/manager-updates";

export class ManagerUpdateService {
  private update: ManagerUpdateHandle | null = null;
  private state = initialManagerUpdateState;

  constructor(private readonly api: ManagerUpdaterApi, private readonly publish: (state: ManagerUpdateState) => void) {}

  async checkForManagerUpdate(): Promise<ManagerUpdateState> {
    await this.releaseUpdate();
    this.set({ ...initialManagerUpdateState, status: "checking", installedVersion: this.state.installedVersion });
    try {
      const installedVersion = await this.api.getCurrentVersion();
      const update = await this.api.check();
      if (update === null) return this.set({ ...initialManagerUpdateState, status: "up_to_date", installedVersion });
      this.update = update;
      return this.set({ status: "update_available", installedVersion, update: { currentVersion: update.currentVersion, availableVersion: update.version, releaseDate: update.date ?? null, releaseNotes: update.body ?? null }, downloadedBytes: null, totalBytes: null, error: null });
    } catch (error) {
      return this.set({ ...initialManagerUpdateState, status: "failed", installedVersion: this.state.installedVersion, error: managerUpdateErrorMessage(error, "The Manager update check failed.") });
    }
  }

  async installManagerUpdate(): Promise<ManagerUpdateState> {
    const update = this.update;
    if (update === null || this.state.status !== "update_available") return this.set({ ...this.state, status: "failed", error: "No Manager update is available to install." });
    try {
      await update.downloadAndInstall((event) => this.onDownloadEvent(event));
      await this.releaseUpdate();
      return this.set({ ...this.state, status: "installing", error: null });
    } catch (error) {
      await this.releaseUpdate();
      return this.set({ ...this.state, status: "failed", error: managerUpdateErrorMessage(error, "The Manager update could not be installed.") });
    }
  }

  async dismiss(): Promise<ManagerUpdateState> { await this.releaseUpdate(); return this.set({ ...initialManagerUpdateState, installedVersion: this.state.installedVersion }); }
  async dispose(): Promise<void> { await this.releaseUpdate(); }

  private onDownloadEvent(event: DownloadEvent) {
    if (event.event === "Started") this.set({ ...this.state, status: "downloading", downloadedBytes: 0, totalBytes: event.data.contentLength ?? null, error: null });
    else if (event.event === "Progress") this.set({ ...this.state, status: "downloading", downloadedBytes: (this.state.downloadedBytes ?? 0) + event.data.chunkLength, error: null });
    else this.set({ ...this.state, status: "installing", error: null });
  }

  private async releaseUpdate() { const update = this.update; this.update = null; if (update !== null) { try { await update.close(); } catch { /* The native resource is already gone or being handed off. */ } } }
  private set(state: ManagerUpdateState) { this.state = state; this.publish(state); return state; }
}

export function managerUpdateErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") return error.message;
  return fallback;
}

interface ManagerUpdatesContextValue { state: ManagerUpdateState; checkForManagerUpdate(): Promise<ManagerUpdateState>; installManagerUpdate(): Promise<ManagerUpdateState>; dismissManagerUpdate(): Promise<ManagerUpdateState>; }
const ManagerUpdatesContext = createContext<ManagerUpdatesContextValue | null>(null);

export function ManagerUpdatesProvider({ children }: PropsWithChildren) {
  const [state, setState] = useState(initialManagerUpdateState);
  const service = useMemo(() => new ManagerUpdateService(tauriManagerUpdater, setState), []);
  const hasStartedInitialCheck = useRef(false);
  useEffect(() => {
    if (!hasStartedInitialCheck.current) {
      hasStartedInitialCheck.current = true;
      void service.checkForManagerUpdate();
    }
    return () => { void service.dispose(); };
  }, [service]);
  const value = useMemo<ManagerUpdatesContextValue>(() => ({ state, checkForManagerUpdate: () => service.checkForManagerUpdate(), installManagerUpdate: () => service.installManagerUpdate(), dismissManagerUpdate: () => service.dismiss() }), [service, state]);
  return createElement(ManagerUpdatesContext.Provider, { value }, children);
}

export function useManagerUpdates(): ManagerUpdatesContextValue {
  const value = useContext(ManagerUpdatesContext);
  if (value === null) throw new Error("useManagerUpdates must be used within ManagerUpdatesProvider.");
  return value;
}
