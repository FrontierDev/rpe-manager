# RPEngine Manager

Phase 2 persisted-contract validation is documented in [docs/PHASE2_VALIDATION.md](docs/PHASE2_VALIDATION.md).

RPEngine Manager is a Windows desktop application built with Tauri, React,
TypeScript, and Vite. It discovers and stores selected World of Warcraft
installations without modifying their files. Dataset management and catalogue
connectivity are not implemented yet.

## Windows prerequisites

- Windows 10 or later.
- Node.js 22.12 or later and npm. Vite 7 requires Node.js 20.19+ or 22.12+.
- Rust stable with the `x86_64-pc-windows-msvc` target.
- Microsoft C++ Build Tools with the **Desktop development with C++** workload.
- Microsoft Edge WebView2 Runtime.

## Local development

From the repository root, install the locked JavaScript dependencies:

```powershell
npm ci
```

Run the frontend in a browser:

```powershell
npm run dev
```

Run the Tauri desktop application:

```powershell
npm run tauri:dev
```

## Local configuration

The Manager stores its own configuration at
`%APPDATA%\net.esarus.rpengine-manager\configuration.json` on Windows. It
contains Manager preferences and selected WoW installation/account identifiers;
it never stores or modifies WoW SavedVariables.

## WoW discovery

The Manager reads common Windows locations, the readable JSON form of Battle.net
installation information, and previously configured paths. A candidate must
contain a `Data` directory and `Wow.exe` or `Wow-64.exe`; `_retail_`, `_ptr_`,
and `_beta_` folder names only identify the product after that structural check.

Use **Select WoW folder** to choose a custom installation through the native
folder dialog. The selected folder is validated before its exact path is saved
to Manager configuration. Discovery and validation do not write under a WoW
directory.

## Selected-installation inspection

For the configured WoW installation, the Manager reads immediate account
directories under `WTF/Account` and reads `Interface/AddOns/RPEngine2/RPEngine2.toc`.
It reports the RPEngine addon as not installed, installed with a readable
version, damaged, or version unavailable. This inspection reads directory and
metadata information only; it does not open character data or change addon and
WoW files.

## WoW running safety

The Manager checks live processes before future actions modify `Interface/AddOns`
or `WTF`. Exact WoW Retail, PTR, and Beta executable names block those writes;
similarly named unrelated processes do not. The UI supports an explicit recheck,
and the process check never terminates World of Warcraft or changes WoW files.

## Backups

The Manager backup service stores verified timestamped backup sets under its
application-data directory, outside the WoW installation. Each set keeps source
installation and account identifiers, the backup reason, source file name, and
byte length. It verifies copied bytes and metadata before reporting success.
The service is available for later SavedVariables and addon changes; this
version does not modify, restore, or delete WoW files.

If the configuration file is malformed, the backend returns a default
configuration together with an explicit recovery result. The caller can show
that result and decide whether to save a replacement configuration.

## Checks and production builds

```powershell
npm run check
npm run lint
npm run build
npm run rust:fmt
npm run rust:lint
npm run rust:test
npm run tauri:build
```

`npm run tauri:build` creates the Windows bundle using the installed Rust and
WebView2 toolchains.

## Continuous integration

GitHub Actions runs the same validation on every push and pull request:

- TypeScript check, lint, and frontend build;
- Rust formatting, Clippy with warnings denied, and Rust tests;
- Windows Tauri production build.

Node dependencies are installed with `npm ci` from `package-lock.json`. Rust
commands use `Cargo.lock` with Cargo's locked mode, so CI does not update the
resolved dependency set.

## Signed releases and updater

Manager releases use Tauri's signed updater artifacts. The npm, Cargo, and
Tauri configuration versions are one release contract; run `npm run
version:check` to reject a mismatch. A release tag must be exactly
`v<version>`, for example `v0.1.1` for version `0.1.1`.

Generate the long-lived updater signing identity once on a secure maintainer
machine. Do not create the key in this repository:

```powershell
npx tauri signer generate --password "<strong private password>" --write-keys "$env:USERPROFILE\.tauri\rpengine-manager.key"
```

Configure the public key printed by that command as the updater `pubkey` in
`src-tauri/tauri.conf.json`. Configure these GitHub Actions secrets from the
private key material; never commit either value:

- `TAURI_SIGNING_PRIVATE_KEY`: contents of `rpengine-manager.key`.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: password used to generate that key.

The release workflow fails before building a release if either signing secret
is absent. It publishes the signed Windows updater artifact and Tauri's
`latest.json` to the GitHub Release. The Manager verifies update signatures;
there is no unsigned fallback or custom executable replacement path.

Regular CI disables updater-artifact generation only while validating that the
Windows installer builds, because pull requests do not receive release
signing secrets. It never uploads or publishes that installer. Only the
tagged release workflow generates updater artifacts and it requires signing.
