import type { WowModificationSafetyState } from "../models/processes";

export interface ModificationActionState {
  disabled: boolean;
  reason: string | null;
}

/** Use this single state when enabling future AddOns or WTF write actions. */
export function getModificationActionState(
  safetyState: WowModificationSafetyState | null,
): ModificationActionState {
  if (safetyState?.canModifyWowFiles === true) {
    return { disabled: false, reason: null };
  }

  return {
    disabled: true,
    reason:
      "Close World of Warcraft before modifying RPEngine files or SavedVariables.",
  };
}
