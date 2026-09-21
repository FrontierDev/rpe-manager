import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { AppFrame, type ManagerPage } from "../components/AppFrame";
import { loadManagerConfiguration, saveManagerConfiguration } from "../api/configuration";
import { discoveryErrorMessage, discoverWowInstallations, isWowDiscoveryCommandError, selectWowInstallation } from "../api/discovery";
import { discoverSelectedWowInstallation, isLocalDiscoveryCommandError } from "../api/local-discovery";
import { getWowModificationSafetyState } from "../api/processes";
import { getSelectedProtocolState, isProtocolStateCommandError } from "../api/protocol-state";
import type { ManagerConfiguration } from "../models/configuration";
import type { WowInstallationCandidate, WowInstallationProductChoice } from "../models/discovery";
import type { SelectedWowInstallationDiscovery } from "../models/local-discovery";
import type { WowModificationSafetyState } from "../models/processes";
import type { SelectedProtocolState } from "../models/protocol-state";
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
  const [protocolState, setProtocolState] = useState<SelectedProtocolState | null>(null);
  const [protocolErrorMessage, setProtocolErrorMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [productChoices, setProductChoices] = useState<WowInstallationProductChoice[] | null>(null);
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

  const refreshProtocolState = useCallback(async () => {
    setProtocolErrorMessage(null);
    try { setProtocolState(await getSelectedProtocolState()); }
    catch (error) {
      setProtocolState(null);
      setProtocolErrorMessage(isProtocolStateCommandError(error) ? error.message : "Persisted RPEngine protocol state could not be read.");
    }
  }, []);

  const refresh = useCallback(async () => {
    setIsLoading(true); setErrorMessage(null); setProductChoices(null); setLocalDiscovery(null); setProtocolState(null);
    try {
      const [loaded, discovered] = await Promise.all([loadManagerConfiguration(), discoverWowInstallations()]);
      setConfiguration(loaded.configuration); setCandidates(discovered);
      const selected = loaded.configuration.installations.find((installation) => installation.id === loaded.configuration.selectedInstallationId);
      if (selected !== undefined && selected.availability === "available") {
        const [discovery] = await Promise.all([discoverSelectedWowInstallation(), refreshProtocolState()]);
        setLocalDiscovery(discovery);
      }
    } catch (error) {
      setErrorMessage(isWowDiscoveryCommandError(error) || isLocalDiscoveryCommandError(error) ? error.message : "World of Warcraft discovery could not be completed.");
    } finally { setIsLoading(false); }
  }, [refreshProtocolState]);

  useEffect(() => { void refresh(); void recheckWowSafety(); }, [recheckWowSafety, refresh]);

  const chooseInstallation = useCallback(async (path?: string) => {
    setIsSelecting(true); setErrorMessage(null); setProductChoices(null);
    try {
      const selectedPath = path ?? await open({ directory: true, multiple: false, title: "Select a World of Warcraft installation folder" });
      if (selectedPath === null) return;
      if (Array.isArray(selectedPath)) throw new Error("Select one World of Warcraft installation folder.");
      const result = await selectWowInstallation(selectedPath);
      if (result.status === "multiple_products") {
        setProductChoices(result.products);
        return;
      }
      setConfiguration(result.configuration);
      await refresh();
    } catch (error) {
      setErrorMessage(discoveryErrorMessage(error));
    } finally { setIsSelecting(false); }
  }, [refresh]);

  const toggleAccount = useCallback((accountId: string, selected: boolean) => {
    if (configuration === null || localDiscovery === null) return;
    const selectedIds = configuration.selectedAccountIds[localDiscovery.installation.id] ?? [];
    const nextIds = selected ? [...selectedIds, accountId] : selectedIds.filter((id) => id !== accountId);
    const next = selectAccounts({ configuration, recoveryMessage: null }, localDiscovery.installation.id, nextIds).configuration;
    void saveManagerConfiguration(next)
      .then((saved) => { setConfiguration(saved); return refreshProtocolState(); })
      .catch(() => setErrorMessage("Account selection could not be saved."));
  }, [configuration, localDiscovery, refreshProtocolState]);

  const home = <HomePage state={getHomeState(configuration, candidates, errorMessage)} configuration={configuration} candidates={candidates} productChoices={productChoices} safetyState={safetyState} safetyErrorMessage={safetyErrorMessage} errorMessage={errorMessage} isLoading={isLoading} isSelecting={isSelecting} isSafetyChecking={isSafetyChecking} onRefresh={() => void refresh()} onRecheckSafety={() => void recheckWowSafety()} onChooseFolder={() => void chooseInstallation()} onSelectCandidate={(path) => void chooseInstallation(path)} onSelectProduct={(path) => void chooseInstallation(path)} />;
  const content = page === "home" ? home : page === "rpengine" ? <RPEnginePage discovery={localDiscovery} protocolState={protocolState} protocolErrorMessage={protocolErrorMessage} onRefreshProtocol={() => void refresh()} /> : <SettingsPage configuration={configuration} discovery={localDiscovery} onToggleAccount={toggleAccount} />;
  return <AppFrame page={page} onNavigate={setPage}>{content}<footer className="app-footer"><span>RPEngine Manager</span><span className="footer-separator">/</span><span>Windows desktop</span><span className="footer-build">LOCAL BUILD</span></footer></AppFrame>;
}
