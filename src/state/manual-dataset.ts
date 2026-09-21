import type { SelectedProtocolState } from "../models/protocol-state";

const header = "RPE_DATASET_V1\n";

/** Data-only outer-envelope inspection. RPE remains the canonical validator. */
export function datasetIdFromExport(payload: string): string {
  return datasetDetailsFromExport(payload).id;
}

export interface DatasetImportDetails {
  id: string;
  name: string;
  category: string;
}

export function datasetDetailsFromExport(payload: string): DatasetImportDetails {
  if (!payload.startsWith(header)) throw new Error("Invalid dataset export.");
  const datasetStart = /\bdataset\s*=\s*\{/.exec(payload);
  const dataset = datasetStart && readBalancedTable(payload, datasetStart.index + datasetStart[0].length - 1);
  const id = dataset && topLevelStringField(dataset, "id");
  if (!/\bformat\s*=\s*["']rpe-dataset["']/.test(payload) || !/\bversion\s*=\s*1\b/.test(payload) || !id || id.trim() !== id || id.length > 128 || [...id].some((character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127)) {
    throw new Error("Dataset export has no valid dataset ID.");
  }
  return {
    id,
    name: topLevelStringField(dataset, "name") ?? id,
    category: topLevelStringField(dataset, "datasetType") ?? "Unknown",
  };
}

/** Reads Lua table nesting and quoted strings without evaluating either. */
function readBalancedTable(source: string, openingBrace: number): string | null {
  let depth = 0;
  let quote: "'" | '"' | null = null;
  let escaped = false;
  for (let index = openingBrace; index < source.length; index += 1) {
    const character = source[index];
    if (quote !== null) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === "'" || character === '"') { quote = character; continue; }
    if (character === "{") depth += 1;
    if (character === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(openingBrace + 1, index);
    }
  }
  return null;
}

function topLevelStringField(table: string, field: string): string | null {
  let depth = 0;
  let quote: "'" | '"' | null = null;
  let escaped = false;
  for (let index = 0; index < table.length; index += 1) {
    const character = table[index];
    if (quote !== null) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === "'" || character === '"') { quote = character; continue; }
    if (character === "{") { depth += 1; continue; }
    if (character === "}") { depth -= 1; continue; }
    if (depth !== 0 || !table.startsWith(field, index)) continue;
    const before = table[index - 1];
    const after = table[index + field.length];
    if ((before && /[A-Za-z0-9_]/.test(before)) || (after && /[A-Za-z0-9_]/.test(after))) continue;
    const match = new RegExp(`^${field}\\s*=\\s*(["'])([^"'\\r\\n]+)\\1`).exec(table.slice(index));
    if (match) return match[2];
  }
  return null;
}

export async function manualCatalogueId(datasetId: string): Promise<string> {
  const bytes = new TextEncoder().encode(datasetId);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return `manual.${[...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

export function nextManualRevision(state: SelectedProtocolState | null, catalogueId: string): number {
  const revisions = new Set((state?.accounts ?? []).flatMap((account) => account.installedPackages
    .filter((item) => item.catalogueId === catalogueId)
    .map((item) => item.revision)));
  if (revisions.size > 1) throw new Error("Selected accounts have different installed revisions. Reconcile them before importing.");
  const [revision] = revisions;
  return revision === undefined ? 1 : revision + 1;
}

export async function sha256(payload: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(payload));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}
