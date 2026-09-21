export interface WowModificationSafetyState {
  isWowRunning: boolean;
  matchingProcessNames: string[];
  canModifyWowFiles: boolean;
}
