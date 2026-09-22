import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";
import { verifyReleaseVersion } from "./verify-version.mjs";

function fixture(packageVersion, cargoVersion, cargoLockVersion, tauriVersion) {
  const root = mkdtempSync(join(tmpdir(), "rpengine-version-"));
  mkdirSync(join(root, "src-tauri"));
  writeFileSync(join(root, "package.json"), JSON.stringify({ version: packageVersion }));
  writeFileSync(join(root, "src-tauri", "Cargo.toml"), `[package]\nversion = "${cargoVersion}"\n`);
  writeFileSync(join(root, "src-tauri", "Cargo.lock"), `[[package]]\nname = "rpengine-manager"\nversion = "${cargoLockVersion}"\n`);
  writeFileSync(join(root, "src-tauri", "tauri.conf.json"), JSON.stringify({ version: tauriVersion }));
  return root;
}

test("accepts matching npm, Cargo, and Tauri versions", () => {
  const root = fixture("0.1.1", "0.1.1", "0.1.1", "0.1.1");
  try { assert.equal(verifyReleaseVersion(root, "v0.1.1"), "0.1.1"); } finally { rmSync(root, { recursive: true, force: true }); }
});

test("rejects a Cargo version that drifts from the package version", () => {
  const root = fixture("0.1.1", "0.1.0", "0.1.1", "0.1.1");
  try { assert.throws(() => verifyReleaseVersion(root), /src-tauri\/Cargo\.toml=0\.1\.0/); } finally { rmSync(root, { recursive: true, force: true }); }
});

test("rejects a Cargo lock version that drifts from the package version", () => {
  const root = fixture("0.1.1", "0.1.1", "0.1.0", "0.1.1");
  try { assert.throws(() => verifyReleaseVersion(root), /src-tauri\/Cargo\.lock=0\.1\.0/); } finally { rmSync(root, { recursive: true, force: true }); }
});
