import { invoke } from "@tauri-apps/api/core";

export interface QueueRulesetRequest {
  requestId: string;
  payload: string;
}

export interface QueueRulesetReport {
  operation: { requestId: string; operation: "install_ruleset" };
  accounts: Array<{ accountId: string; status: "queued" | "failed"; error?: { code: string; message: string } }>;
}

export function queueRuleset(request: QueueRulesetRequest): Promise<QueueRulesetReport> {
  return invoke<QueueRulesetReport>("queue_install_ruleset", { request });
}
