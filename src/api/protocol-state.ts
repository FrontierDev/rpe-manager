import { invoke } from "@tauri-apps/api/core";
import type {
  ProtocolStateCommandError,
  SelectedProtocolState,
} from "../models/protocol-state";

export async function getSelectedProtocolState(): Promise<SelectedProtocolState> {
  return invoke<SelectedProtocolState>("get_selected_protocol_state");
}

export function isProtocolStateCommandError(
  error: unknown,
): error is ProtocolStateCommandError {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error
  );
}
