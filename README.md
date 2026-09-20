# RPEngine Manager

RPEngine Manager is a Windows desktop application built with Tauri, React,
TypeScript, and Vite. The current version establishes the application shell and
backend boundaries; it does not yet discover or modify World of Warcraft files,
manage datasets, or connect to a catalogue.

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

## Checks and production builds

```powershell
npm run check
npm run lint
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri:build
```

`npm run tauri:build` creates the Windows bundle using the installed Rust and
WebView2 toolchains.
