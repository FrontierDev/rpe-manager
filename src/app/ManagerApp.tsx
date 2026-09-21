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
import { ManagerPage as ManagerPageContent } from "../pages/Manager/ManagerPage";
import { RPEnginePage } from "../pages/RPEngine/RPEnginePage";
import { SettingsPage } from "../pages/Settings/SettingsPage";
import { SafetyPanel } from "../components/SafetyPanel";
import { queueRuleset } from "../api/rulesets";
import { queueDataset, queueDatasetRemoval as queueDatasetRemovalRequest } from "../api/datasets";
import type { DatasetRow } from "../models/datasets";
import { datasetDetailsFromExport, manualCatalogueId, nextManualRevision, sha256 } from "../state/manual-dataset";

export function ManagerApp() {
  const [page, setPage] = useState<ManagerPage>("manager");
  const [configuration, setConfiguration] = useState<ManagerConfiguration | null>(null);
  const [candidates, setCandidates] = useState<WowInstallationCandidate[]>([]);
  const [localDiscovery, setLocalDiscovery] = useState<SelectedWowInstallationDiscovery | null>(null);
  const [safetyState, setSafetyState] = useState<WowModificationSafetyState | null>(null);
  const [protocolState, setProtocolState] = useState<SelectedProtocolState | null>(null);
  const [protocolErrorMessage, setProtocolErrorMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [safetyErrorMessage, setSafetyErrorMessage] = useState<string | null>(null);
  const [productChoices, setProductChoices] = useState<WowInstallationProductChoice[] | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSelecting, setIsSelecting] = useState(false);
  const [isSafetyChecking, setIsSafetyChecking] = useState(true);
  const [manualDatasetDetails, setManualDatasetDetails] = useState<Record<string, { name: string; category: string }>>({});

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
      // Discovery owns default-installation selection. Load after it completes
      // so the UI sees the persisted choice in the same refresh.
      const discovered = await discoverWowInstallations();
      const loaded = await loadManagerConfiguration();
      setConfiguration(loaded.configuration); setCandidates(discovered);
      const selected = loaded.configuration.installations.find((installation) => installation.id === loaded.configuration.selectedInstallationId);
      if (selected !== undefined && selected.availability === "available") {
        const [discovery] = await Promise.all([discoverSelectedWowInstallation(), refreshProtocolState()]);
        setLocalDiscovery(discovery);
        // Configurations created before account defaults existed have no map
        // entry. Persist the initial selection once; an empty entry remains an
        // explicit choice and is never replaced on later refreshes.
        if (!(selected.id in loaded.configuration.selectedAccountIds)) {
          const next = selectAccounts(
            { configuration: loaded.configuration, recoveryMessage: null },
            selected.id,
            discovery.accounts.map((account) => account.id),
          ).configuration;
          const saved = await saveManagerConfiguration(next);
          setConfiguration(saved);
          await refreshProtocolState();
        }
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

  const queueImportedRuleset = useCallback(async (payload: string) => {
    if (!payload.startsWith("RPE_RULESET_V1\n") || payload.length <= "RPE_RULESET_V1\n".length) {
      throw new Error("Paste a complete RPE ruleset export.");
    }
    const report = await queueRuleset({ requestId: crypto.randomUUID(), payload });
    await refreshProtocolState();
    const failed = report.accounts.filter((account) => account.status === "failed");
    return failed.length === 0
      ? `Queued for ${report.accounts.length} account(s). Reload RPE to import it.`
      : `Queued for ${report.accounts.length - failed.length}; ${failed.map((account) => account.error?.code ?? "failed").join(", ")}.`;
  }, [refreshProtocolState]);

  const queueImportedDataset = useCallback(async (payload: string) => {
    const installationId = configuration?.selectedInstallationId;
    if (!installationId || !(configuration?.selectedAccountIds[installationId]?.length)) {
      throw new Error("Select at least one account before importing.");
    }
    if (safetyState?.canModifyWowFiles !== true) {
      throw new Error("Close World of Warcraft before importing a dataset.");
    }
    const details = datasetDetailsFromExport(payload);
    const catalogueId = await manualCatalogueId(details.id);
    const revision = nextManualRevision(protocolState, catalogueId);
    const report = await queueDataset({
      requestId: crypto.randomUUID(),
      catalogueId,
      datasetId: details.id,
      revision,
      hash: await sha256(payload),
      payload,
    });
    setManualDatasetDetails((current) => ({ ...current, [details.id]: { name: details.name, category: details.category } }));
    await refreshProtocolState();
    const failed = report.accounts.filter((account) => account.status === "failed");
    return failed.length === 0
      ? `Queued for ${report.accounts.length} account(s). Reload RPE to install it.`
      : `Failed: ${failed.map((account) => account.error?.message ?? account.error?.code ?? "queue error").join("; ")}`;
  }, [configuration, protocolState, refreshProtocolState, safetyState]);

  const queueDatasetRemoval = useCallback(async (row: DatasetRow) => {
    const installationId = configuration?.selectedInstallationId;
    if (!installationId || !(configuration?.selectedAccountIds[installationId]?.length)) {
      throw new Error("Select at least one account before removing a dataset.");
    }
    if (safetyState?.canModifyWowFiles !== true) {
      throw new Error("Close World of Warcraft before removing a dataset.");
    }
    if (!row.catalogueId || !row.datasetId || !row.hash || row.installedRevision === null) {
      throw new Error("This dataset does not have a removable installed identity.");
    }
    const report = await queueDatasetRemovalRequest({ requestId: crypto.randomUUID(), catalogueId: row.catalogueId, datasetId: row.datasetId, revision: row.installedRevision, hash: row.hash });
    await refreshProtocolState();
    const failed = report.accounts.filter((account) => account.status === "failed");
    return failed.length === 0 ? `Removal queued for ${report.accounts.length} account(s).` : `Failed: ${failed.map((account) => account.error?.message ?? "queue error").join("; ")}`;
  }, [configuration, refreshProtocolState, safetyState]);

  const content = page === "manager"
    ? <ManagerPageContent configuration={configuration} discovery={localDiscovery} protocolState={protocolState} errorMessage={errorMessage} productChoices={productChoices} isLoading={isLoading} isSelecting={isSelecting} onChooseFolder={() => void chooseInstallation()} onSelectInstallation={(path) => { if (path) void chooseInstallation(path); }} onSelectProduct={(path) => void chooseInstallation(path)} onToggleAccount={toggleAccount} onQueueRuleset={queueImportedRuleset} onQueueDataset={queueImportedDataset} onQueueDatasetRemoval={queueDatasetRemoval} onRefresh={() => void refresh()} manualDatasetDetails={manualDatasetDetails} />
    : <><SettingsPage configuration={configuration} discovery={localDiscovery} candidates={candidates} onToggleAccount={toggleAccount} /><section className="page-section"><SafetyPanel state={safetyState} errorMessage={safetyErrorMessage} isChecking={isSafetyChecking} onRecheck={() => void recheckWowSafety()} /></section><RPEnginePage discovery={localDiscovery} protocolState={protocolState} protocolErrorMessage={protocolErrorMessage} onRefreshProtocol={() => void refresh()} /></>;
  return <AppFrame page={page} onNavigate={setPage}>{content}</AppFrame>;
}
