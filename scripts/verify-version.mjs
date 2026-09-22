import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
/* global console, process */

export function verifyReleaseVersion(root = resolve(import.meta.dirname, ".."), tag = process.env.RELEASE_TAG) {
  const read = (path) => readFileSync(resolve(root, path), "utf8");
  const packageVersion = JSON.parse(read("package.json")).version;
  const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(read("src-tauri/Cargo.toml"))?.[1];
  const tauriVersion = JSON.parse(read("src-tauri/tauri.conf.json")).version;
  const versions = { "package.json": packageVersion, "src-tauri/Cargo.toml": cargoVersion, "src-tauri/tauri.conf.json": tauriVersion };

  if (!Object.values(versions).every((version) => version === packageVersion)) {
    throw new Error(`Manager release versions disagree: ${Object.entries(versions).map(([file, version]) => `${file}=${version ?? "missing"}`).join(", ")}`);
  }
  if (tag && tag !== `v${packageVersion}`) throw new Error(`Release tag ${tag} does not match packaged version v${packageVersion}.`);
  return packageVersion;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) console.log(`Release version ${verifyReleaseVersion()} is consistent.`);
