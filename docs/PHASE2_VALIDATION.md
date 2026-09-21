# Phase 2 validation

This guide validates the persisted boundary between RPEngine Manager and the RPE addon. It does not introduce an alternate importer, deleter, or dataset representation: RPE must perform every dataset change through its canonical `Database.ImportDataset` and `Database.DeleteDataset` paths.

## Automated contract coverage

Run from the repository root:

```powershell
npm run rust:test
```

`src-tauri/tests/phase2_contract.rs` uses the v1 request and persisted-result fixtures in [`docs/protocol-v1/`](protocol-v1/). It verifies that Manager queues each request with a backup, preserves the unrelated SavedVariables roots, and reconciles the persisted install, update, removal, and terminal-failure states. The test records observed addon output as fixtures; it never simulates RPE dataset import or deletion.

The fixture payloads are protocol-envelope markers. They validate Manager hashing and queue shape, but they are not canonical RPE datasets. The in-game validation below requires payloads exported by RPE itself.

## Prerequisites

- A development RPE addon containing the protocol work from `rpe2#388`, `#389`, and `#390` is installed in a disposable WoW installation.
- WoW is closed before each Manager queue action.
- A dedicated test account is selected in Manager Settings.
- Two real `RPE_DATASET_V1` payloads are exported through RPE's canonical serializer for one disposable dataset ID. The second payload changes the dataset while retaining the same dataset ID.
- Record the exact UTF-8 bytes and SHA-256 hash for each payload. Preserve LF line endings; hash the bytes before copying the payload into the Manager request.

Use a unique catalogue ID, request ID, and disposable dataset ID. The example fixture identities are illustrative only.

## Install

1. In Manager Settings, select only the disposable account.
2. Queue `install_dataset` through the development Tauri command `queue_install_dataset`. Supply a new `requestId`, the catalogue ID, dataset ID, revision `14`, the SHA-256 for the exact payload bytes, and the first canonical payload. The command resolves the configured account itself; it accepts no package-provided path.
3. Confirm the command reports `queued` and a backup ID. In `RPEngine2.lua`, `RPEngineManagerDB.pendingOperations` contains the complete request and unrelated roots, including `RPEngineDatasetDB`, are unchanged. The Manager backup contains the pre-queue bytes.
4. Start WoW with RPE enabled. Let RPE load the request and verify in the addon that the canonical import path is used. Verify the dataset is normalized, active, and participates in normal dependency behaviour.
5. Exit WoW so it saves SavedVariables. Reload the RPEngine page in Manager only after this save.
6. Confirm the account reports protocol v1, no pending install request, a terminal `succeeded` result for the request ID, and one installed package with revision `14` and the supplied hash.

Manager’s result is persisted state only. While WoW is running, Manager labels the disk snapshot stale and does not confirm live success.

The current Phase 2 shell does not expose a catalogue form. In a development build, invoke the narrow command with the selected account already saved in Manager configuration:

```ts
await invoke("queue_install_dataset", {
  request: {
    requestId: "a-new-stable-request-id",
    catalogueId: "phase2-test-package",
    datasetId: "the-canonical-fixture-dataset-id",
    revision: 14,
    hash: "sha256-of-the-exact-payload-bytes",
    payload: "RPE_DATASET_V1\n...canonical RPE payload...",
  },
});
```

## Update

1. With the installed package from the install scenario still present, queue a second `install_dataset` request with the same catalogue ID and dataset ID, revision `15`, and the second canonical payload/hash.
2. Start WoW, let RPE process the request, and verify the existing dataset is replaced through canonical import rather than duplicated.
3. Exit/save WoW and refresh Manager.
4. Confirm the terminal result succeeds and the sole manifest entry has revision `15` and the second hash.

## Removal

1. Queue `remove_dataset` with a new request ID and the exact manifest catalogue ID, dataset ID, revision, and hash.
2. Start WoW and verify RPE runs canonical `Database.DeleteDataset` behaviour.
3. Exit/save WoW and refresh Manager.
4. Confirm the removal result succeeds and `installedPackages` no longer has that catalogue ID.

## Required failure checks

| Case | Action | Expected persisted or Manager result |
| --- | --- | --- |
| Invalid envelope | Submit a payload without the exact `RPE_DATASET_V1\n` header. | Manager rejects it before backup/write; no fallback mutation occurs. |
| Invalid canonical payload | Submit a header-bearing payload that RPE's canonical importer rejects. | RPE records `failed` with `invalid_payload_format` or `import_rejected`; the manifest remains unchanged. |
| Dataset ID mismatch | Queue a canonical payload while declaring another dataset ID. | RPE records `failed` with `dataset_id_mismatch`; no manifest entry is advanced. |
| Completed request replay | Queue an already terminal request ID. | Manager rejects it before writing. If a saved replay is loaded by RPE, the existing result wins and the replay is consumed without reapplying it. |
| Unsupported protocol | Set `protocolVersion = 2` in a disposable SavedVariables copy. | Manager reports `unsupported`; neither side migrates or overwrites the root. |
| Malformed root | Use an unclosed or otherwise malformed `RPEngineManagerDB` table. | Manager reports a parse error and does not queue anything. |
| WoW running | Start WoW, then attempt a Manager queue request. | Every selected account reports `wow_running`; no backup or SavedVariables write occurs. |
| Backup or staging failure | Run the deterministic transaction tests. | The original source survives; replacement is never attempted before a verified backup and staged parse succeed. |
| Removal identity mismatch | Queue removal with an incorrect revision or hash. | RPE records `failed` with `installed_package_mismatch`; the manifest remains present. |

The `failed-dataset-id-mismatch.json` and `failed-removal-identity-mismatch.json` fixtures demonstrate the expected persisted terminal shapes. They are read/reconciliation fixtures, not substitutes for the addon validation.

## Reset

Close WoW. Remove the disposable dataset through the successful removal scenario, then restore the pre-test `RPEngine2.lua` from the Manager backup if the dedicated account must return to its exact original state. Keep the backup until the validation record is accepted. Do not reset by editing `RPEngineDatasetDB.datasets`.
