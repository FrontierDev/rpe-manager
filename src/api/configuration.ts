import { invoke } from "@tauri-apps/api/core";
import type {
  ConfigurationCommandError,
  ConfigurationLoad,
  ManagerConfiguration,
} from "../models/configuration";

export async function loadManagerConfiguration(): Promise<ConfigurationLoad> {
  return invoke<ConfigurationLoad>("load_manager_configuration");
}

export async function saveManagerConfiguration(
  configuration: ManagerConfiguration,
): Promise<ManagerConfiguration> {
  return invoke<ManagerConfiguration>("save_manager_configuration", {
    configuration,
  });
}

export function isConfigurationCommandError(
  value: unknown,
): value is ConfigurationCommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}
