import type { ManagerConfiguration } from "../models/configuration";

export interface ConfigurationState {
  configuration: ManagerConfiguration;
  recoveryMessage: string | null;
}

export function selectInstallation(
  state: ConfigurationState,
  installationId: string | null,
): ConfigurationState {
  if (
    installationId !== null &&
    !state.configuration.installations.some(
      (installation) => installation.id === installationId,
    )
  ) {
    throw new Error(`Unknown installation identifier: ${installationId}`);
  }

  return {
    ...state,
    configuration: {
      ...state.configuration,
      selectedInstallationId: installationId,
    },
  };
}

export function selectAccounts(
  state: ConfigurationState,
  installationId: string,
  accountIds: string[],
): ConfigurationState {
  if (
    !state.configuration.installations.some(
      (installation) => installation.id === installationId,
    )
  ) {
    throw new Error(`Unknown installation identifier: ${installationId}`);
  }

  const selectedAccountIds = [...new Set(accountIds)].sort();
  const nextSelections = { ...state.configuration.selectedAccountIds };

  // An empty array is an explicit user choice, not an absent preference.
  nextSelections[installationId] = selectedAccountIds;

  return {
    ...state,
    configuration: {
      ...state.configuration,
      selectedAccountIds: nextSelections,
    },
  };
}
