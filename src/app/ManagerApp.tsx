import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { AppFrame, type ManagerPage } from "../components/AppFrame";
import { loadManagerConfiguration, saveManagerConfiguration } from "../api/configuration";
import { discoverWowInstallations, isWowDiscoveryCommandError, selectWowInstallation } from "../api/discovery";
import { discoverSelectedWowInstallation, isLocalDiscoveryCommandError } from "../api/local-discovery";
import { getWowModificationSafetyState } from "../api/processes";
import type { ManagerConfiguration } from "../models/configuration";
import type { WowInstallationCandidate } from "../models/discovery";
import type { SelectedWowInstallationDiscovery } from "../models/local-discovery";
import type { WowModificationSafetyState } from "../models/processes";
import { selectAccounts } from "../state/configuration";
import { getHomeState } from "./manager-state";
import { HomePage } from "../pages/Home/HomePage";
import { RPEnginePage } from "../pages/RPEngine/RPEnginePage";
import { SettingsPage } from "../pages/Settings/SettingsPage";

export function ManagerApp() {
  const [page, setPage] = useState<ManagerPage>("home");
  const [configuration, setConfiguration] = useState<ManagerConfiguration | null>(null);
  const [candidates, setCandidates] = useState<WowInstallationCandidate[]>([]);
  const [localDiscovery, setLocalDiscovery] = useState<SelectedWowInstallationDiscovery | null>(null);
  const [safetyState, setSafetyState] = useState<WowModificationSafetyState | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [safetyErrorMessage, setSafetyErrorMessage] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSelecting, setIsSelecting] = useState(false);
  const [isSafetyChecking, setIsSafetyChecking] = useState(true);

  const recheckWowSafety = useCallback(async () => {
    setIsSafetyChecking(true);
    setSafetyErrorMessage(null);
    try { setSafetyState(await getWowModificationSafetyState()); }
    catch { setSafetyErrorMessage("World of Warcraft running state could not be checked."); }
    finally { setIsSafetyChecking(false); }
  }, []);

  const refresh = useCallback(async () => {
    setIsLoading(true); setErrorMessage(null); setLocalDiscovery(null);
    try {
      const [loaded, discovered] = await Promise.all([loadManagerConfiguration(), discoverWowInstallations()]);
      setConfiguration(loaded.configuration); setCandidates(discovered);
      const selected = loaded.configuration.installations.find((installation) => installation.id === loaded.configuration.selectedInstallationId);
      if (selected !== undefined && selected.availability === "available") setLocalDiscovery(await discoverSelectedWowInstallation());
    } catch (error) {
      setErrorMessage(isWowDiscoveryCommandError(error) || isLocalDiscoveryCommandError(error) ? error.message : "World of Warcraft discovery could not be completed.");
    } finally { setIsLoading(false); }
  }, []);

  useEffect(() => { void refresh(); void recheckWowSafety(); }, [recheckWowSafety, refresh]);

  const chooseInstallation = useCallback(async (path?: string) => {
    setIsSelecting(true); setErrorMessage(null);
    try {
      const selectedPath = path ?? await open({ directory: true, multiple: false, title: "Select a World of Warcraft installation folder" });
      if (selectedPath === null) return;
      if (Array.isArray(selectedPath)) throw new Error("Select one World of Warcraft installation folder.");
      setConfiguration(await selectWowInstallation(selectedPath));
      await refresh();
    } catch (error) {
      setErrorMessage(isWowDiscoveryCommandError(error) ? error.message : error instanceof Error ? error.message : "The selected folder could not be configured.");
    } finally { setIsSelecting(false); }
  }, [refresh]);

  const toggleAccount = useCallback((accountId: string, selected: boolean) => {
    if (configuration === null || localDiscovery === null) return;
    const selectedIds = configuration.selectedAccountIds[localDiscovery.installation.id] ?? [];
    const nextIds = selected ? [...selectedIds, accountId] : selectedIds.filter((id) => id !== accountId);
    const next = selectAccounts({ configuration, recoveryMessage: null }, localDiscovery.installation.id, nextIds).configuration;
    void saveManagerConfiguration(next).then(setConfiguration).catch(() => setErrorMessage("Account selection could not be saved."));
  }, [configuration, localDiscovery]);

  const home = <HomePage state={getHomeState(configuration, candidates, errorMessage)} configuration={configuration} candidates={candidates} safetyState={safetyState} safetyErrorMessage={safetyErrorMessage} errorMessage={errorMessage} isLoading={isLoading} isSelecting={isSelecting} isSafetyChecking={isSafetyChecking} onRefresh={() => void refresh()} onRecheckSafety={() => void recheckWowSafety()} onChooseFolder={() => void chooseInstallation()} onSelectCandidate={(path) => void chooseInstallation(path)} />;
  const content = page === "home" ? home : page === "rpengine" ? <RPEnginePage discovery={localDiscovery} /> : <SettingsPage configuration={configuration} discovery={localDiscovery} onToggleAccount={toggleAccount} />;
  return <AppFrame page={page} onNavigate={setPage}>{content}<footer className="app-footer"><span>RPEngine Manager</span><span className="footer-separator">/</span><span>Windows desktop</span><span className="footer-build">LOCAL BUILD</span></footer></AppFrame>;
}
