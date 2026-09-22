# RPEngine External Management Protocol v1

- **Status:** Contract for Phase 2 implementation
- **SavedVariable:** `RPEngineManagerDB`
- **Protocol version:** `1`

This document defines the persisted contract shared by RPEngine Manager and the RPEngine addon. It is normative: **MUST**, **MUST NOT**, and **MAY** describe implementation requirements. The Manager queues requests; RPE validates and applies them through its canonical dataset APIs.

## 1. Ownership and scope

`RPEngineManagerDB` is the only cross-process communication surface in protocol v1. The Manager may create this table when it is absent and append requests to `pendingOperations`. The Manager MUST NOT write or rewrite `installedPackages` or `operationResults`; those fields are owned by RPE. When editing an existing SavedVariables file, the Manager MUST preserve all valid addon-owned fields.

The Manager MUST NOT directly write `RPEngineDatasetDB.datasets` or any other RPE dataset database table. RPE remains authoritative for payload validation, normalization, activation, dependency calculation, and deletion. Install payloads retain the existing `RPE_DATASET_V1` format and are passed through RPE's canonical import path. The Manager MUST treat payloads as data and MUST NOT execute them as Lua.

The protocol does not define catalogue HTTP requests, password handling, downloads, dependency resolution, or SavedVariables parsing. Filesystem paths and timestamps are not part of request identity.

## 2. Representation and root table

The SavedVariable has this exact v1 shape:

```lua
RPEngineManagerDB = {
    protocolVersion = 1,
    pendingOperations = {},
    installedPackages = {},
    operationResults = {},
}
```

All four fields are required. The table MUST NOT contain additional root fields in v1. Lua field names and casing are exact.

| Field | Type | Meaning |
| --- | --- | --- |
| `protocolVersion` | integer, exactly `1` | Contract version. |
| `pendingOperations` | dense, 1-based array of operation tables | Manager-owned FIFO queue. An empty queue is `{}`. |
| `installedPackages` | map from `catalogueId` string to installed-package table | RPE-owned package manifest. An empty map is `{}`. |
| `operationResults` | map from `requestId` string to terminal-result table | RPE-owned, append-only idempotency/result ledger. An empty map is `{}`. |

In the JSON fixtures, arrays use `[]`, maps use `{}`, and object property names match Lua fields. The fixtures illustrate the same data contract; they are not a second wire format.

If the SavedVariable is absent, either side MAY initialize the exact empty v1 shape. If it is present but malformed, or its `protocolVersion` is absent, non-integer, or not `1`, both sides MUST fail closed: do not rewrite the root, consume requests, or infer another version. The addon reports a diagnostic; the Manager surfaces an unsupported or malformed protocol state. A version change requires an explicit protocol revision and migration plan.

V1 is a closed schema. Unknown root or persisted-record fields are malformed; implementations MUST NOT silently discard them. A v1 producer MUST emit only the fields defined here.

## 3. Shared scalar rules

These rules apply wherever the named fields occur:

| Field | Required value |
| --- | --- |
| `requestId` | 1–128 ASCII characters matching `[A-Za-z0-9][A-Za-z0-9._:-]{0,127}`. It is opaque, stable across retries, and unique for the lifetime of this `RPEngineManagerDB`. A UUID is suitable. Reuse is prohibited even after failure. |
| `catalogueId` | 1–128 lowercase ASCII characters matching `[a-z0-9][a-z0-9._-]{0,127}`. |
| `datasetId` | 1–128 UTF-8 bytes, with no NUL, control characters, or leading/trailing whitespace. It MUST equal the stable ID declared by the RPE dataset payload for an install. |
| `revision` | Positive integer from `1` through `2,147,483,647`. |
| `hash` | Exactly 64 lowercase hexadecimal characters: SHA-256. |

Each request ID identifies one logical operation, not a file, account path, or time. Retrying a request means preserving its entire operation table and `requestId`; starting a distinct user-requested operation requires a new ID.

## 4. Pending operation records

The queue is a dense 1-based sequence. The Manager appends to its end; RPE processes from index `1` in order. There is no priority, parallel processing, or timestamp ordering. Each record has exactly the fields for its operation below; unknown fields are invalid.

### 4.1 `install_dataset`

Required fields:

| Field | Type and validation |
| --- | --- |
| `requestId` | Shared request ID rule. |
| `operation` | Exact string `install_dataset`. |
| `catalogueId` | Shared catalogue ID rule. |
| `datasetId` | Shared dataset ID rule; must agree with the payload's dataset ID. |
| `revision` | Shared positive revision rule. |
| `hash` | SHA-256 of the exact UTF-8 bytes of `payload`, before any newline or other normalization. |
| `payload` | Non-empty string beginning with the exact bytes `RPE_DATASET_V1\n`. The rest is opaque to the Manager and MUST be validated by RPE's canonical dataset importer. |

The Manager MUST verify `hash` against `payload` before adding the operation to the queue. RPE validates the hash syntax and MUST still validate/import the payload canonically; the hash never substitutes for payload validation. RPE records the supplied verified hash in the package manifest.

An update is another `install_dataset` for the same `catalogueId` and `datasetId` with a higher `revision`. If a catalogue ID is already associated with another dataset ID, RPE rejects the operation. RPE MUST reject a lower revision. The same revision and hash is a successful no-op; the same revision with a different hash is a conflict. A higher revision is imported through the canonical import path before the manifest is advanced.

### 4.2 `remove_dataset`

Required fields:

| Field | Type and validation |
| --- | --- |
| `requestId` | Shared request ID rule. |
| `operation` | Exact string `remove_dataset`. |
| `catalogueId` | Shared catalogue ID rule. |
| `datasetId` | Shared dataset ID rule. |
| `revision` | The exact currently installed package revision expected to be removed. |
| `hash` | The exact currently installed manifest hash expected to be removed. |

`payload` is forbidden. Before removal, RPE MUST confirm that `installedPackages[catalogueId]` has the same `datasetId`, `revision`, and `hash`. A missing entry or identity mismatch is a failure; it MUST NOT cause RPE to guess which dataset to delete. On a match, RPE calls its canonical deletion path. RPE removes the manifest entry only after canonical deletion succeeds. Dependency policy remains RPE's responsibility; a rejected deletion is reported as a failed result.

## 5. Installed package manifest

`installedPackages` is a map keyed by `catalogueId`. Every entry has exactly this shape:

```lua
installedPackages["esarus-core"] = {
    packageType = "dataset",
    datasetId = "f82db71a",
    revision = 14,
    hash = "<64 lowercase SHA-256 hex characters>",
    installedAt = 1789940000,
}
```

| Field | Required type and meaning |
| --- | --- |
| `packageType` | Exact string `dataset`. |
| `datasetId` | Shared dataset ID rule. |
| `revision` | Shared positive revision rule. |
| `hash` | SHA-256 for the installed package payload, as defined for install requests. |
| `installedAt` | Integer Unix time in UTC seconds, set by RPE when a new revision is successfully installed. It is descriptive metadata and is never used for ordering or identity. |

The manifest records Manager package state separately from authored dataset content. It MUST NOT be embedded in or exported as part of `RPE_DATASET_V1`.

## 6. Operation results

`operationResults` is a map keyed by `requestId`. Each key MUST exactly equal the value's `requestId`, and every key/value ID must follow the shared request ID rule. Each value is a terminal record with this shape:

```lua
operationResults["550e8400-e29b-41d4-a716-446655440000"] = {
    requestId = "550e8400-e29b-41d4-a716-446655440000",
    operation = "install_dataset", -- or "remove_dataset" or "unknown"
    status = "succeeded", -- or "failed"
    catalogueId = "esarus-core",
    datasetId = "f82db71a",
    revision = 14,
    hash = "<64 lowercase SHA-256 hex characters>",
}
```

`requestId`, `operation`, and `status` are always required. `operation` is `install_dataset`, `remove_dataset`, or the reserved value `unknown` when the operation is missing or not a string. For a recognized operation, each valid identity field (`catalogueId`, `datasetId`, `revision`, `hash`) is copied into the result. Those identity fields are omitted when unavailable or malformed; Lua `nil` is represented by omission, never by JSON `null`.

For a failure, `status` is `failed` and an `error` table is required:

```lua
error = {
    code = "invalid_payload_format",
    detail = "Payload must begin with RPE_DATASET_V1 followed by LF.",
}
```

`error.code` is a stable machine-readable value from the list below. `error.detail` is non-empty human-readable UTF-8 text of at most 1024 bytes. It may give a version-specific explanation, so clients MUST use `code`, not parse `detail`, for decisions. Error details MUST NOT contain passwords, credentials, or complete package payloads.

For success, `status` is `succeeded`, all four package identity fields are required, and `error` is absent. A successful remove result identifies the package revision and hash that were removed.

V1 error codes:

| Code | Meaning |
| --- | --- |
| `invalid_field` | A required field is missing, has the wrong type, violates a scalar rule, or an operation has an extra field. |
| `unsupported_operation` | `operation` is a string but is not a v1 operation. |
| `invalid_payload_format` | The install payload is empty or lacks the required `RPE_DATASET_V1\n` header. |
| `dataset_id_mismatch` | The declared dataset ID differs from the ID validated from the payload. |
| `catalogue_dataset_id_conflict` | This catalogue ID is already associated with a different dataset ID. |
| `stale_revision` | The requested revision is lower than the installed revision. |
| `revision_hash_conflict` | The requested revision equals the installed revision but its hash differs. |
| `package_not_installed` | No installed-package manifest entry exists for a removal request. |
| `installed_package_mismatch` | The removal identity does not exactly match the installed manifest entry. |
| `import_rejected` | RPE's canonical importer rejected the dataset. |
| `remove_rejected` | RPE's canonical deletion path rejected the removal. |
| `internal_error` | RPE could not complete the operation for another explicit internal reason. |

Root-level protocol errors (malformed root or unsupported `protocolVersion`) cannot safely be attached to a request. RPE reports these through its diagnostic/logging surface and leaves the root untouched; it MUST NOT fabricate a request result.

## 7. Consumption, ordering, and idempotency

RPE begins processing after SavedVariables are loaded and its canonical dataset APIs are initialized during addon startup (including `/reload`). Every client follows the same lifecycle; there is no host-specific protocol path. RPE processes the queue sequentially from its first entry. For each entry with a syntactically valid request ID:

1. If `operationResults` already contains that ID, treat the entry as a replay: do not inspect it for execution, do not change the stored result or manifest, and remove this queue occurrence.
2. Otherwise validate the operation record. If its request ID is valid but a field is invalid or the operation is unsupported, create one failed terminal result with the appropriate code and consume the entry without calling an importer or deleter.
3. For a valid operation, call the canonical RPE import or deletion path. Update the manifest only after success. A rejected operation leaves the manifest unchanged.
4. Add exactly one terminal result under the request ID, then remove the first queue entry. Record the result before removing the queue entry.

The first queued occurrence of an ID determines its result. Any later occurrence, even if its fields differ, is a replay and is discarded after the first result exists. A pre-existing result is immutable. A failed request MUST NOT be retried under the same ID; a deliberate retry is a new request with a new ID.

Results are retained indefinitely in v1. Neither Manager nor RPE may prune or reuse a result ID: retaining every terminal ID is required to prevent an old successful or failed request from being applied again. This is a deliberate storage tradeoff; a future bounded-retention policy would require a protocol-version change and an explicit replay horizon.

If an entry is not a table or has a missing, non-string, or syntactically invalid `requestId`, RPE cannot create an idempotent result. It MUST stop at that queue position, leave that entry and all later entries untouched, and report a diagnostic. Earlier entries already completed in FIFO order remain completed. The Manager must repair or remove the malformed request with a backed-up SavedVariables edit before processing can continue.

## 8. Compatibility and failure handling

- An absent root is initialized to the empty v1 shape.
- A root with the exact supported version and valid root field types is processed under this document.
- A malformed root, unknown root field, malformed manifest/result map, or unsupported protocol version is not migrated or partially processed. The addon reports the problem and leaves the complete root unchanged. The Manager refuses to append to it and surfaces the actual problem.
- A malformed operation with a valid request ID is a terminal `invalid_field` failure (or `unsupported_operation` where applicable) and is consumed in FIFO order.
- Root incompatibility is not recorded in `operationResults`, since mutating an untrusted/unknown contract to report that incompatibility would not be safe.
- A result already present for a request ID always wins over a reappearing pending copy, regardless of the copy's contents.

Implementations MUST NOT fall back to editing `RPEngineDatasetDB.datasets` if protocol processing fails.

### 8.1 `install_ruleset`

`install_ruleset` is a local manual import request. It uses the normal v1
envelope fields with `catalogueId = "local"`, `datasetId = "ruleset-import"`,
and `revision = 1`; `hash` is SHA-256 of the exact UTF-8 payload bytes. The
required payload begins with `RPE_RULESET_V2\n` or `RPE_RULESET_V2\r\n`; the remaining bytes are opaque
to Manager.

Manager validates only that outer marker and exact hash, then appends the
operation through `RPEngineManagerDB` using the existing backup and atomic-write
transaction. It MUST NOT execute or parse the payload, accept paths from it, or
write `RPEngineRulesetDB`.

RPE consumes the request exactly once with `Database.ImportRuleset(payload,
options)`. Canonical acceptance or rejection is recorded as the usual terminal
success or failure result, including a machine-readable error code and detail.

## 9. Shared fixtures

Machine-readable v1 examples are in [`protocol-v1/`](protocol-v1/):

- `valid-install-request.json` — install envelope with a matching payload hash.
- `valid-remove-request.json` — removal whose expected identity matches the installed manifest.
- `successful-result.json` — consumed install with a terminal success result and manifest entry.
- `failed-result.json` — consumed install with an explicit machine code and human-readable detail.
- `installed-package-manifest.json` — standalone populated manifest example.
- `duplicate-request-id-replay.json` — completed request reappears in the queue; RPE discards it without reapplying it or changing its result.

- `valid-update-request.json` — later revision envelope for the same catalogue and dataset IDs.
- `update-successful-result.json` — consumed update with its revised manifest entry.
- `remove-successful-result.json` — consumed removal with no remaining manifest entry.
- `failed-dataset-id-mismatch.json` — canonical payload ID mismatch terminal failure.
- `failed-removal-identity-mismatch.json` — removal mismatch that retains its manifest entry.

The install fixture's payload body is an opaque protocol-envelope marker, not a claim about the inner dataset schema. It has the required header and matching SHA-256 so both projects can validate envelope handling. End-to-end importer tests must substitute a payload emitted by RPE's canonical `RPE_DATASET_V1` serializer; the protocol contract intentionally does not duplicate that schema.
