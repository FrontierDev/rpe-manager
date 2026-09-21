export type ProtocolAvailability =
  | "absent"
  | "available"
  | "malformed"
  | "source_unavailable"
  | "unsupported";

export type ProtocolStateErrorCode =
  | "malformed"
  | "source_read"
  | "source_not_file"
  | "unsupported";

export interface ProtocolStateError {
  code: ProtocolStateErrorCode;
  message: string;
}

export interface PendingOperation {
  requestId: string;
  operation: "install_dataset" | "remove_dataset" | "install_ruleset";
  catalogueId: string;
  datasetId: string;
  revision: number;
  hash: string;
  hasPayload: boolean;
}

export interface OperationFailure {
  code: string;
  detail: string;
}

export interface OperationResult {
  requestId: string;
  operation: "install_dataset" | "remove_dataset" | "install_ruleset" | "unknown";
  status: "succeeded" | "failed";
  catalogueId?: string;
  datasetId?: string;
  revision?: number;
  hash?: string;
  error?: OperationFailure;
}

export interface InstalledPackageRecord {
  catalogueId: string;
  packageType: "dataset";
  datasetId: string;
  revision: number;
  hash: string;
  installedAt: number;
}

export interface AccountProtocolState {
  accountId: string;
  availability: ProtocolAvailability;
  protocolVersion?: number;
  pendingOperations: PendingOperation[];
  operationResults: OperationResult[];
  installedPackages: InstalledPackageRecord[];
  error?: ProtocolStateError;
}

export interface SelectedProtocolState {
  isStale: boolean;
  staleReason?: string;
  accounts: AccountProtocolState[];
}

export interface ProtocolStateCommandError {
  code: "configuration" | "protocol_state";
  message: string;
}
