export const configurationSchemaVersion = 1;

export type WowProduct = "retail" | "ptr" | "beta";

export type InstallationAvailability = "available" | "unavailable";

export interface WowInstallation {
  id: string;
  product: WowProduct | null;
  path: string;
  availability: InstallationAvailability;
}

export interface ManagerPreferences {
  backupRetention: number;
  updateCheckingEnabled: boolean;
}

export interface ManagerConfiguration {
  schemaVersion: number;
  installations: WowInstallation[];
  selectedInstallationId: string | null;
  selectedAccountIds: Record<string, string[]>;
  preferences: ManagerPreferences;
  /** Reserved for a later catalogue integration. */
  catalogueUrl: string | null;
}

export interface ConfigurationRecovery {
  code: "invalid_configuration";
  message: string;
}

export interface ConfigurationLoad {
  configuration: ManagerConfiguration;
  recovery: ConfigurationRecovery | null;
}

export interface ConfigurationCommandError {
  code: "configuration_path" | "path_inspection" | "read" | "write";
  message: string;
}

export const defaultManagerPreferences: ManagerPreferences = {
  backupRetention: 10,
  updateCheckingEnabled: true,
};

export function createDefaultManagerConfiguration(): ManagerConfiguration {
  return {
    schemaVersion: configurationSchemaVersion,
    installations: [],
    selectedInstallationId: null,
    selectedAccountIds: {},
    preferences: { ...defaultManagerPreferences },
    catalogueUrl: null,
  };
}
