# RPEngine Manager and Dataset Distribution
## Product Design Document

**Status:** Draft  
**Date:** 20 September 2026  
**Products:** RPEngine Manager, RPEngine 2, Esarus website  
**Repositories:**
- FrontierDev/rpe-manager — desktop application
- FrontierDev/rpe2 — existing World of Warcraft addon
- FrontierDev/esarus — existing website and dataset catalogue

**Initial desktop target:** Windows  
**Desktop technology:** Tauri + React + TypeScript + Vite  
**Website:** Existing Esarus React/TypeScript/Vite application  
**Backend:** Cloudflare-hosted API and storage

---

# 1. Purpose

RPEngine currently supports dataset import and export through its in-game Data Editor.

This works for direct sharing, but becomes increasingly unsuitable as datasets become:

- larger;
- more numerous;
- guild-specific;
- player-authored;
- dependent on other datasets;
- revised frequently;
- shared across multiple characters or accounts.

The objective of this project is to introduce a proper distribution and update system for RPEngine.

The system consists of:

1. **RPEngine Manager**
   - Desktop application.
   - Installs and updates RPEngine itself.
   - Discovers WoW installations.
   - Discovers RPEngine installations and SavedVariables.
   - Installs, updates and removes RPE datasets.
   - Handles backups and safety checks.
   - Communicates with the online RPE dataset catalogue.

2. **RPEngine Catalogue**
   - Hosted through esarus.net/rpengine.
   - Allows users to browse available datasets.
   - Categorises datasets by guild and player.
   - Supports public and password-protected datasets.
   - Displays dependencies, versions and metadata.
   - Can open datasets directly in RPEngine Manager.

3. **RPEngine Addon Integration**
   - Provides a stable external-management interface.
   - Processes dataset installation requests using RPE's existing canonical import logic.
   - Records package-management metadata separately from authored dataset content.
   - Never requires the desktop Manager to understand RPE's complete internal SavedVariables schema.

The intended user experience is comparable to a small package manager rather than manual copy-and-paste dataset distribution.

---

# 2. Core Product Principle

The desktop application must **not become a second implementation of the RPE database**.

RPE itself remains authoritative for:

- dataset validation;
- schema normalization;
- dependency calculation;
- dataset activation;
- replacement of existing dataset versions;
- deletion semantics;
- future dataset migrations.

The desktop application is responsible for:

- discovering files;
- downloading packages;
- verifying packages;
- backing up SavedVariables;
- communicating desired external operations to RPE;
- displaying package state.

The separation is:

~~~
RPEngine Manager
        |
        | external package operation
        v
Stable external-management SavedVariable
        |
        v
RPEngine
        |
        +-- validate
        +-- normalize
        +-- import/update/remove
        +-- activate
        +-- recompute dependencies
        v
RPEngineDatasetDB
~~~

This boundary must remain stable even if RPE's internal database schema changes.

---

# 3. Repository Architecture

The system remains split across three repositories.

## 3.1 FrontierDev/rpe-manager

New desktop application repository.

Responsibilities:

- Tauri desktop application;
- local WoW discovery;
- local RPE discovery;
- process detection;
- SavedVariables backup;
- SavedVariables external-operation writes;
- package catalogue client;
- dataset installation/update/removal UI;
- RPEngine addon installation/update;
- local configuration;
- diagnostics and logs;
- custom rpe:// URI handling;
- application self-update if later enabled.

It must not contain the RPE addon itself or the Esarus website.

## 3.2 FrontierDev/rpe2

Existing repository.

Required additions are limited to the stable external-management contract.

Responsibilities:

- consume Manager-originated package requests;
- import datasets through the existing canonical dataset import path;
- remove Manager-controlled datasets through canonical addon APIs;
- record successful Manager installations;
- expose installed package state in a Manager-readable manifest;
- reject invalid or incompatible package requests.

The normal in-game dataset editor remains fully functional.

Datasets do not require RPEngine Manager in order to exist.

## 3.3 FrontierDev/esarus

Existing website repository.

Responsibilities:

- RPEngine landing page;
- Manager download links;
- public dataset catalogue;
- guild and player categories;
- dataset detail pages;
- protected-dataset unlock flow;
- Manager deep links;
- manual dataset download as a fallback;
- associated backend/API integration.

The existing guild website remains otherwise independent.

---

# 4. Terminology

## RPEngine Manager

The desktop application.

The UI may use the shorter product name:

~~~
RPE Manager
~~~

but the formal product name should remain:

~~~
RPEngine Manager
~~~

## Package

A remotely distributed unit managed by RPEngine Manager.

Initial package types:

~~~
rpe-addon
dataset
~~~

Future package types may be introduced without changing the dataset model.

## Dataset ID

The existing stable RPE dataset identifier.

Example:

~~~
f82db71a
~~~

This identifies the RPE dataset itself.

It must remain stable between updates.

## Catalogue ID

A server-side package identifier.

Example:

~~~
esarus-core
~~~

This identifies the published package in the distribution system.

A catalogue ID maps to one RPE dataset ID.

## Revision

A positive integer representing the published revision of a package.

Example:

~~~
revision 14
~~~

Revision belongs to the package catalogue, not the authored RPE dataset.

Uploading a changed package increments its revision.

---

# 5. High-Level Architecture

~~~
                        esarus.net
                            |
                 +----------+----------+
                 |                     |
             Web Catalogue         Package API
                                       |
                              +--------+--------+
                              |                 |
                         Metadata DB       Package Store
                              |                 |
                              +--------+--------+
                                       | HTTPS
                                       v
                              RPEngine Manager
                                       |
                    +------------------+------------------+
                    |                                     |
             Interface/AddOns                          WTF
                    |                                     |
            RPEngine installation             External Manager state
                                                          |
                                                          v
                                                    RPEngine addon
                                                          |
                                                          v
                                                 RPEngineDatasetDB
~~~

---

# 6. Technology

## Desktop

Use:

- Tauri;
- Rust for privileged/local-system logic;
- React;
- TypeScript;
- Vite;
- modular CSS or another lightweight project-specific visual system.

Rust owns:

- filesystem access;
- process inspection;
- hashing;
- archive extraction;
- atomic file replacement;
- backups;
- path discovery;
- OS integration;
- URI protocol handling.

React/TypeScript owns:

- catalogue UI;
- installed-package UI;
- navigation;
- progress and status display;
- settings;
- error presentation.

Frontend code must not receive broader filesystem permissions than required.

---

# 7. Supported Platforms

Phase 1 targets:

~~~
Windows
~~~

The architecture should avoid Windows-only assumptions where inexpensive to do so.

Possible later targets:

~~~
macOS
Linux
~~~

The initial project is not required to ship these platforms.

---

# 8. Manager First-Run Experience

On first launch:

~~~
Welcome to RPEngine Manager

[ Locate World of Warcraft ]
~~~

The Manager should first attempt automatic discovery.

If successful:

~~~
World of Warcraft detected

Retail
C:\Program Files (x86)\World of Warcraft\_retail_

RPEngine
Installed: Yes
Version: 2.0.alpha5

Accounts
2 detected

[ Continue ]
~~~

If automatic discovery fails:

~~~
World of Warcraft could not be located automatically.

[ Select World of Warcraft Folder ]
~~~

The user must always be able to override automatic detection.

---

# 9. WoW Installation Discovery

The Manager should identify candidate WoW installations from:

- common Windows installation paths;
- Battle.net installation information where reliably available;
- previously selected Manager paths;
- user-selected paths.

A valid WoW installation should be recognised structurally rather than solely from its folder name.

Supported installations should include distinct roots such as:

~~~
_retail_
_ptr_
_beta_
~~~

where applicable.

Each discovered installation receives its own Manager identity.

Example conceptual configuration:

~~~json
{
  "id": "wow-retail-main",
  "product": "retail",
  "path": "C:\\Program Files (x86)\\World of Warcraft\\_retail_"
}
~~~

---

# 10. Account Discovery

For each installation, inspect:

~~~
WTF/Account/
~~~

and identify available accounts.

The Manager UI should support:

~~~
Retail
  Account A
  Account B

PTR
  Account A
~~~

Datasets may be installed for one or several accounts.

The application must not assume only one WoW account exists.

---

# 11. RPEngine Detection

The Manager detects RPEngine from:

~~~
Interface/AddOns/RPEngine2/
~~~

and reads appropriate addon metadata to determine the installed version.

Display states:

~~~
Not installed
Installed
Update available
Installation damaged
Unsupported version
~~~

The Manager should not infer a healthy installation merely because the directory exists.

---

# 12. Installing RPEngine

RPEngine Manager should ultimately be the preferred way to install RPEngine itself.

Flow:

~~~
RPEngine not installed

Latest version:
2.0.alpha6

[ Install RPEngine ]
~~~

The Manager:

1. downloads the official package;
2. verifies its declared cryptographic hash;
3. validates the archive;
4. backs up an existing installation where appropriate;
5. extracts to the correct AddOns location;
6. verifies the resulting structure;
7. reports success.

The application must never extract arbitrary archive paths outside the intended RPEngine directory.

---

# 13. Updating RPEngine

When an update exists:

~~~
RPEngine

Installed
2.0.alpha5

Available
2.0.alpha6

[ Update ]
~~~

Updates should use staged replacement:

~~~
download
   |
verify
   |
extract to temporary directory
   |
validate
   |
backup current installation
   |
replace installation
   |
verify
~~~

An incomplete update must not leave RPEngine half-replaced.

---

# 14. Dataset Catalogue

Datasets are grouped primarily by publisher type.

Top-level catalogue navigation:

~~~
Featured
Guilds
Players
Installed
Updates
~~~

Example:

~~~
Guilds

Esarus
  Esarus Core
  Esarus Campaign
  Esarus Professions

Gnomeregan Air Service
  GAS Campaign
~~~

Player category:

~~~
Players

Ortellus
  Encounter Toolkit
  Personal Campaign

Cybercog
  ...
~~~

Search must operate across:

- package name;
- publisher;
- description;
- tags.

---

# 15. Dataset Catalogue Record

A catalogue entry should conceptually contain:

~~~json
{
  "catalogueId": "esarus-core",
  "packageType": "dataset",
  "datasetId": "f82db71a",
  "name": "Esarus Core",
  "description": "...",
  "publisherType": "guild",
  "publisherId": "esarus",
  "publisherName": "Esarus",
  "revision": 14,
  "rpeVersionMinimum": "...",
  "rpeVersionMaximum": null,
  "protected": false,
  "dependencies": [
    "rpe-core"
  ],
  "hash": "...",
  "publishedAt": "...",
  "updatedAt": "..."
}
~~~

Exact server representation may differ.

The distinction between:

~~~
catalogueId
datasetId
revision
~~~

must be preserved.

---

# 16. Dataset Payload

The Manager distributes the same logical dataset format supported by RPE itself.

The canonical dataset payload remains:

~~~
RPE_DATASET_V1
~~~

The package distribution system must not invent a separate representation of RPE gameplay content.

Catalogue metadata wraps the payload externally.

---

# 17. Dataset Dependencies

The catalogue should publish known package dependencies.

Example:

~~~
Esarus Campaign
Requires:

✓ RPE Core
✓ Esarus Core
○ Esarus Professions
~~~

When installing a package:

~~~
Install Esarus Campaign?

The following required datasets will also be installed:

• Esarus Core
• RPE Core

[ Cancel ] [ Install 3 Packages ]
~~~

Dependency resolution must reject:

- unresolved dependencies;
- dependency cycles;
- incompatible versions.

RPE itself remains responsible for validating its internal dataset references after import.

The Manager's dependency model is an installation convenience, not a replacement for RPE's dependency system.

---

# 18. Dataset Installation

The Manager must not directly mutate:

~~~lua
RPEngineDatasetDB.datasets
~~~

Instead it writes an external operation for RPE to process.

Conceptual operation:

~~~lua
{
    operation = "install_dataset",

    catalogueId = "esarus-core",
    revision = 14,

    datasetId = "f82db71a",

    hash = "...",

    payload = "RPE_DATASET_V1\n..."
}
~~~

RPE then imports payload using its canonical import implementation.

For the current RPE architecture, this means ultimately passing the payload through:

~~~lua
Database.ImportDataset(...)
~~~

or its future canonical replacement.

---

# 19. External Manager SavedVariables

Introduce one dedicated SavedVariable owned by the integration contract.

Proposed:

~~~
RPEngineManagerDB
~~~

It must not contain ordinary player Profile state.

Conceptual structure:

~~~lua
RPEngineManagerDB = {
    protocolVersion = 1,

    pendingOperations = {},

    installedPackages = {},

    operationResults = {},
}
~~~

Exact structure should be finalized in the RPE implementation plan.

---

# 20. Pending Operations

Example:

~~~lua
pendingOperations = {
    {
        requestId = "...",
        operation = "install_dataset",

        catalogueId = "esarus-core",
        revision = 14,
        datasetId = "f82db71a",

        hash = "...",
        payload = "...",
    }
}
~~~

Operations must have stable unique request IDs.

Supported initial operations:

~~~
install_dataset
remove_dataset
~~~

An update uses:

~~~
install_dataset
~~~

with the same dataset ID and a later catalogue revision.

The existing RPE import behaviour already naturally replaces a dataset with the same ID.

---

# 21. Operation Processing

At addon startup:

~~~
RPEngineManagerDB pending operation
        |
validate envelope
        |
validate payload
        |
run canonical RPE operation
        |
      success?
      /     \
    yes     no
     |       |
  record   record
 installed failure
 package   reason
     |
remove pending operation
~~~

Processing must be idempotent.

RPE must not repeatedly reinstall a successfully processed operation after /reload.

---

# 22. Installed Package Manifest

RPE records successful Manager-controlled installations separately from the dataset itself.

Example:

~~~lua
installedPackages = {
    ["esarus-core"] = {
        packageType = "dataset",
        datasetId = "f82db71a",
        revision = 14,
        hash = "...",
        installedAt = 1789940000,
    },
}
~~~

This is Manager metadata.

It must not be serialized into the RPE dataset export itself.

---

# 23. Manual In-Game Changes

Users remain free to edit a Manager-installed dataset inside RPE.

The Manager must therefore distinguish:

~~~
installed package revision
~~~

from:

~~~
current local dataset content
~~~

Where practical, RPE should expose or record a deterministic content hash.

The Manager can then display:

~~~
Esarus Core
Installed revision 14
Modified locally
~~~

Updating a locally modified dataset must require confirmation:

~~~
This dataset has been modified locally.

Updating will replace the local copy with revision 15.

[ Cancel ]
[ Update and Replace ]
~~~

Automatic silent destruction of user edits is prohibited.

---

# 24. Dataset Updates

Update detection uses package revision:

~~~
installed revision 14
remote revision 15
~~~

Result:

~~~
Update available
~~~

The Manager should not use modification dates as the authoritative version comparison.

---

# 25. Dataset Removal

For Manager-managed datasets:

~~~
[ Remove ]
~~~

must create a removal operation for RPE.

RPE then removes the dataset through its own canonical deletion path.

Before removal, the Manager should warn about installed dependants:

~~~
Esarus Core is required by:

• Esarus Campaign
• Esarus Professions

Remove all dependent packages as well?

[ Cancel ]
[ Remove 3 Packages ]
~~~

Removal must not directly delete arbitrary tables from the SavedVariables file.

---

# 26. Public Datasets

Public packages may be fetched without authentication.

Dataset detail:

~~~
Esarus Core
By Esarus

Revision 14
Public

[ Install ]
~~~

The actual package hash must be verified after download.

---

# 27. Password-Protected Datasets

A dataset may be protected by a publisher-supplied password.

Catalogue listings may still expose non-sensitive metadata:

~~~
Private Campaign
By Esarus
🔒 Protected
~~~

Attempting installation produces:

~~~
This dataset requires a password.

Password
[________________]

[ Unlock ]
~~~

The client sends the password over HTTPS to the package API.

The backend verifies the password.

Only after successful verification is the dataset payload returned.

---

# 28. Password Storage

Passwords must never be stored in plaintext by the server.

The server stores a suitable salted password hash.

The catalogue response must not contain:

- the password;
- the password hash;
- enough information for the static website bundle to bypass validation.

Protected package payloads must not exist as publicly accessible static website assets.

---

# 29. Remembering Protected Access

Phase 1 does not require permanent password storage.

An unlocked package may remain authorised for the current Manager session.

A later phase may introduce securely stored access tokens.

If credentials or tokens are persisted, OS-provided secure credential storage should be used rather than plaintext configuration files.

---

# 30. Publisher Categories

Initial publisher types:

~~~
guild
player
~~~

Example guild publisher:

~~~
publisherType = guild
publisherId = esarus
publisherName = Esarus
~~~

Example player publisher:

~~~
publisherType = player
publisherId = ortellus
publisherName = Ortellus
~~~

A dataset belongs to one primary publisher.

Tags can provide additional organization.

---

# 31. Publishing

Package publication is administratively separate from ordinary package installation.

Initial publication may be handled through controlled administrative tooling rather than exposing public user registration.

A publisher workflow eventually needs to support:

~~~
Create package
Upload RPE_DATASET_V1
Validate payload
Assign publisher
Set description
Set visibility
Set password if required
Set compatibility
Publish revision
~~~

Publishing a changed payload increments the catalogue revision.

The original dataset ID must remain stable for an update.

A payload with a different dataset ID should normally be considered a different package rather than an update.

---

# 32. Server-Side Validation

Before accepting a published dataset, server-side validation should check at minimum:

- payload format;
- package size;
- catalogue metadata;
- declared dataset ID;
- revision integrity;
- duplicate catalogue IDs;
- hash generation.

Where complete RPE schema validation is impractical on the server, final authoritative validation remains inside RPE itself.

---

# 33. Website Integration

Add:

~~~
/rpengine
~~~

to the Esarus website.

The page should contain:

~~~
RPEngine introduction

[ Download RPEngine Manager ]

Browse Datasets
  Featured
  Guilds
  Players
~~~

Dataset pages should support:

~~~
[ Install with RPEngine Manager ]
[ Manual Download ]
~~~

Manual download remains important for:

- unsupported operating systems;
- users who do not want the Manager;
- debugging;
- recovery.

---

# 34. Custom URI Scheme

Register:

~~~
rpe://
~~~

with the operating system.

Example:

~~~
rpe://dataset/esarus-core
~~~

Website action:

~~~
Install with RPEngine Manager
~~~

opens the Manager directly to:

~~~
Esarus Core
[ Install ]
~~~

The URI must identify the catalogue package only.

Passwords, authentication tokens and complete dataset contents must never be embedded in the URI.

---

# 35. Website Fallback

If the Manager does not open, the website should provide:

~~~
RPEngine Manager did not open.

[ Download RPEngine Manager ]
[ Download Dataset Manually ]
~~~

The website must not become unusable without the desktop application.

---

# 36. Cloud Backend

The existing Esarus site may remain primarily static while gaining dynamic RPE endpoints.

Required backend responsibilities:

~~~
catalogue metadata
package lookup
protected-package authentication
package download authorization
package publication
version manifests
RPEngine release manifests
~~~

Suggested storage split:

~~~
D1
  structured package/publisher metadata

R2
  dataset payloads
  release archives
~~~

The precise Cloudflare implementation may evolve without affecting Manager or RPE contracts.

---

# 37. API

Conceptual endpoints:

~~~
GET  /api/rpe/catalogue
GET  /api/rpe/packages/:catalogueId
POST /api/rpe/packages/:catalogueId/unlock
GET  /api/rpe/packages/:catalogueId/download

GET  /api/rpe/releases/latest
GET  /api/rpe/releases/:version
~~~

Publishing/admin endpoints are separate.

The Manager must consume a versioned API contract.

Example:

~~~
/api/rpe/v1/...
~~~

Breaking API changes require a new API version.

---

# 38. Package Integrity

Every downloadable package must have a cryptographic hash in trusted metadata.

Download process:

~~~
retrieve metadata
        |
download package
        |
calculate hash locally
        |
compare
        |
      match?
      /   \
    yes   no
     |     |
 continue reject
~~~

A hash mismatch must never produce a warning-only installation.

---

# 39. Manager Local Configuration

Manager-owned local state should contain only Manager configuration.

Example:

~~~json
{
  "wowInstallations": [],
  "selectedInstallation": "...",
  "selectedAccounts": [],
  "catalogueUrl": "https://www.esarus.net",
  "preferences": {}
}
~~~

Do not duplicate complete WoW SavedVariables inside Manager configuration.

Backups are stored separately.

---

# 40. WoW Process Detection

Before modifying either:

~~~
Interface/AddOns
~~~

or:

~~~
WTF
~~~

the Manager should detect whether World of Warcraft is running.

If running:

~~~
World of Warcraft is currently running.

Close World of Warcraft before modifying RPEngine files.
Changes to SavedVariables while the game is running may be overwritten.

[ Recheck ]
~~~

Modification operations remain disabled until the process is no longer detected.

---

# 41. Backups

Every SavedVariables modification requires a backup.

The Manager must back up the complete affected file before writing.

Example:

~~~
backups/
  2026-09-20T221400/
    RPEngine2.lua
~~~

A rotating backup policy may remove old backups later.

The most recent backups must be visible to the user.

---

# 42. Atomic SavedVariables Writes

The Manager must not overwrite a SavedVariables file in-place incrementally.

Required sequence:

~~~
read original
    |
create backup
    |
construct modified content
    |
write temporary file
    |
validate temporary file
    |
atomic replace where supported
~~~

If replacement fails, the original file remains available.

---

# 43. SavedVariables Parser Scope

The Manager should parse only enough SavedVariables syntax to safely manipulate the dedicated Manager integration table.

It should avoid becoming a general-purpose implementation of RPE's internal Lua database.

Where possible, the integration table should be structured to make this manipulation simple and deterministic.

The implementation must not execute SavedVariables as Lua code.

Parsing and serialization must treat it as data.

---

# 44. Multiple Accounts

Installation scope must be explicit.

Example:

~~~
Install Esarus Core to:

Retail

☑ Account One
☐ Account Two
☑ Account Three

[ Install ]
~~~

The Manager processes each target independently.

Partial success must be reported explicitly:

~~~
Account One     Installed
Account Three   Failed: SavedVariables unavailable
~~~

---

# 45. Multiple WoW Installations

Each installation maintains independent state.

Example:

~~~
Retail
  RPEngine 2.0.alpha6
  8 datasets

PTR
  RPEngine 2.0.alpha5
  3 datasets
~~~

The user must always be able to see which installation is currently selected.

---

# 46. Main Manager UI

Recommended primary navigation:

~~~
Home
Datasets
RPEngine
Backups
Settings
~~~

---

# 47. Home Page

Example:

~~~
RPEngine Manager

Retail
RPEngine 2.0.alpha5

Updates available

RPEngine                 2.0.alpha5 → 2.0.alpha6
Esarus Core              revision 14 → 15
Esarus Campaign          revision 7 → 8

[ Update All ]
~~~

Also show:

~~~
3 updates available
12 datasets installed
Last checked: ...
~~~

---

# 48. Datasets Page

Sections:

~~~
Installed
Updates
Browse
Guilds
Players
~~~

Dataset cards should expose:

- name;
- publisher;
- description;
- installed revision;
- available revision;
- protected/public state;
- dependency status;
- local modification state;
- install/update/remove action.

---

# 49. Dataset Detail Page

Example:

~~~
Esarus Campaign

Esarus
Guild Dataset

Large campaign dataset containing...

Installed revision
7

Latest revision
8

Dependencies
✓ RPE Core
✓ Esarus Core

Updated
20 September 2026

[ Update ]
~~~

Protected packages display their lock state.

---

# 50. RPEngine Page

Example:

~~~
RPEngine

Installation
Retail

Installed version
2.0.alpha5

Latest version
2.0.alpha6

Install path
C:\...\Interface\AddOns\RPEngine2

[ Update ]
[ Open Folder ]
~~~

If absent:

~~~
RPEngine is not installed.

[ Install RPEngine ]
~~~

---

# 51. Backups Page

Display:

~~~
20 Sep 2026 22:14
Before dataset update
Account One
[ Restore ]

20 Sep 2026 18:02
Before RPEngine update
Retail
[ Restore ]
~~~

Restoration must itself make a backup before replacing current state.

---

# 52. Settings

Initial settings:

~~~
WoW installations
Selected installation
Accounts
Catalogue server
Backup retention
Update checking
~~~

Advanced/debug settings should be separated from ordinary user settings.

---

# 53. Update All

Update All resolves the complete operation set before modifying anything.

Example:

~~~
Update All

RPEngine
  2.0.alpha5 → 2.0.alpha6

Datasets
  Esarus Core        14 → 15
  Esarus Campaign     7 → 8

[ Cancel ]
[ Update ]
~~~

Dependencies determine installation order.

RPEngine itself should normally update before datasets that require the newer addon version.

---

# 54. RPE Version Compatibility

Packages may declare:

~~~
minimum RPE version
maximum RPE version
~~~

If a dataset requires a newer RPE version:

~~~
Esarus Campaign requires RPEngine 2.0.alpha6 or later.

RPEngine 2.0.alpha5 is installed.

[ Update RPEngine and Install ]
~~~

The Manager must not install a known-incompatible package without explicit developer/debug override.

---

# 55. Manager Compatibility

The API may also declare a minimum Manager version.

An outdated Manager should produce:

~~~
RPEngine Manager must be updated before this package can be installed.
~~~

Protocol versions must be explicit rather than inferred.

---

# 56. Manager Self-Update

Self-update is desirable but does not need to block Phase 1.

A later phase may allow:

~~~
RPEngine Manager update available
1.2.0 → 1.3.0

[ Update ]
~~~

Manager self-update infrastructure must be separate from RPE addon updates.

---

# 57. Logging

The Manager should maintain structured local logs covering:

~~~
startup
WoW discovery
RPE discovery
catalogue requests
downloads
hash validation
backups
SavedVariables writes
addon installs
dataset operations
errors
~~~

Logs must not contain:

- dataset passwords;
- authentication secrets;
- complete protected payloads unless explicitly running a developer diagnostic mode.

---

# 58. Diagnostics

Provide:

~~~
Settings → Diagnostics
~~~

with:

~~~
Manager version
Operating system
Detected WoW installations
Selected paths
Detected accounts
RPE versions
API connectivity
Latest operation results

[ Export Diagnostic Report ]
~~~

The report should redact credentials and protected dataset contents.

---

# 59. Error Philosophy

Avoid silent fallbacks.

Bad:

~~~
Could not locate configured WoW path → silently use another installation
~~~

Correct:

~~~
Configured WoW installation is unavailable.

[ Locate Installation ]
~~~

Bad:

~~~
Dataset hash failed → attempt installation anyway
~~~

Correct:

~~~
Downloaded package failed integrity verification.
Installation was cancelled.
~~~

Bad:

~~~
RPE operation failed → mark package installed
~~~

Correct:

~~~
Installation request was written, but RPE rejected the package:
Unsupported dataset payload.
~~~

The Manager should expose the actual failure state.

---

# 60. Security Boundary

The Manager should be treated as privileged software because it can modify local files.

Required constraints:

- filesystem access limited to user-approved/configured locations;
- archive extraction protected against path traversal;
- remote payload hashes verified;
- HTTPS required for catalogue operations;
- no execution of downloaded dataset contents;
- no arbitrary shell commands supplied by the server;
- no remote-controlled filesystem paths;
- no plaintext password persistence;
- no secrets in logs;
- no automatic modification while WoW is running.

A dataset is data, not executable Manager code.

---

# 61. RPE Dataset Security

RPE dataset content remains capable only of whatever the RPE dataset schema supports.

The distribution service must never introduce a package mechanism that causes arbitrary Lua from a dataset publisher to be executed.

The package service distributes RPE's existing serialized dataset format only.

---

# 62. Manual Import Compatibility

RPE's existing manual import/export functionality remains supported.

A user must always be able to:

~~~
download dataset
        |
copy/import manually through RPE
~~~

Manager functionality is additive.

It must not create a proprietary dataset format required for ordinary RPE use.

---

# 63. Existing Manually Imported Datasets

Datasets already installed manually should appear in the Manager where possible.

If their dataset ID matches a catalogue package:

~~~
Esarus Core

Installed manually
Matching catalogue package found

[ Adopt into Manager ]
~~~

Adoption records package metadata without reinstalling the dataset if content is compatible.

If the Manager cannot prove the local dataset matches the catalogue revision:

~~~
Matching package found, but the local dataset differs.

[ Keep Local ]
[ Replace with Published Version ]
~~~

---

# 64. Package Ownership Does Not Grant Local Authority

A publisher can publish a later revision.

They cannot remotely force it onto a user's computer.

All installation and update actions are user-controlled unless the user explicitly enables future automatic-update functionality.

Password protection controls download access.

It does not grant the publisher remote administration of the user's RPE installation.

---

# 65. Automatic Updates

Automatic dataset updates are out of scope for Phase 1.

The Manager may automatically:

~~~
check for updates
~~~

but applying updates requires user action.

A later option may support:

~~~
Automatically install trusted public dataset updates
~~~

but protected datasets and locally modified datasets require special handling.

---

# 66. Notifications

Optional later feature:

~~~
3 RPEngine updates available
~~~

through desktop notifications.

Notifications should not be required for normal operation.

---

# 67. Proposed rpe-manager Repository Layout

~~~
rpe-manager/
  README.md
  package.json
  vite.config.ts
  tsconfig.json

  src/
    app/
    components/
    pages/
      Home/
      Datasets/
      DatasetDetail/
      RPEngine/
      Backups/
      Settings/

    api/
    hooks/
    models/
    state/
    styles/
    utils/

  src-tauri/
    Cargo.toml

    src/
      main.rs

      commands/
      discovery/
        wow.rs
        accounts.rs
        rpengine.rs

      filesystem/
        backup.rs
        atomic_write.rs
        saved_variables.rs

      packages/
        download.rs
        hash.rs
        archive.rs
        dependencies.rs

      processes/
        wow.rs

      protocol/
        external_manager.rs

      diagnostics/
        logs.rs
        report.rs

  docs/
    PDD.md
    protocol.md
    api.md
~~~

Exact module names may evolve.

Responsibilities should remain separated.

---

# 68. Development Environment

rpe-manager should be an independent VS Code project.

Typical local structure:

~~~
Projects/
  rpe2/
  esarus/
  rpe-manager/
~~~

Normally open:

~~~
rpe-manager/
~~~

directly in VS Code when working on the Manager.

A developer-only multi-root workspace may later include:

~~~
rpe2
esarus
rpe-manager
~~~

for changes affecting the integration contract.

The repositories themselves remain separate.

---

# 69. CI

The Manager repository should have its own GitHub Actions.

Required checks should eventually include:

~~~
TypeScript compile
frontend lint
Rust format
Rust lint / clippy
Rust tests
frontend tests
Tauri build validation
~~~

Release workflows should remain separate from ordinary validation workflows.

---

# 70. Release Artifacts

Initial Windows release should provide a normal installer.

The release process should generate:

~~~
installer
version metadata
cryptographic hash
release manifest
~~~

These artifacts are used by the website and, eventually, Manager self-update.

---

# 71. Phase 1 — Desktop Foundation

Implement FrontierDev/rpe-manager.

Scope:

- Tauri application;
- React/TypeScript UI;
- WoW discovery;
- manual WoW path selection;
- account discovery;
- RPEngine detection;
- WoW process detection;
- local configuration;
- backup infrastructure;
- diagnostics;
- basic application shell.

No remote dataset installation is required yet.

## Acceptance Criteria

~~~
Manager launches on Windows.

A standard Retail WoW installation can be detected.

A custom installation can be selected manually.

Multiple WoW accounts are discovered.

RPEngine installation status and version are shown.

WoW running state is detected.

No WoW or RPE files are modified during discovery.

Manager configuration persists across restart.

Diagnostics correctly report the selected installation.
~~~

---

# 72. Phase 2 — RPE External Management Protocol

Modify FrontierDev/rpe2.

Implement:

- RPEngineManagerDB;
- protocol version;
- pending dataset install operations;
- canonical dataset import processing;
- operation result recording;
- installed package manifest;
- dataset removal operations;
- idempotency;
- content/hash state where required.

Manager implements:

- safe SavedVariables parser;
- backup before write;
- atomic operation insertion;
- reading result and installed-package state.

## Acceptance Criteria

~~~
Manager can queue an RPE_DATASET_V1 payload.

RPE consumes the payload on startup.

The dataset passes through the same canonical import logic as manual imports.

Imported datasets are normalized and activated normally.

Dependencies are recomputed normally.

A repeated completed request is not applied twice.

RPE exposes explicit success/failure state.

Manager never directly writes RPEngineDatasetDB.datasets.

Manager can request canonical dataset removal.

All SavedVariables writes are backed up first.
~~~

---

# 73. Phase 3 — Catalogue and Backend

Modify FrontierDev/esarus.

Implement:

- /rpengine;
- catalogue UI;
- guild categories;
- player categories;
- dataset detail pages;
- search;
- Manager download page;
- versioned package API;
- metadata database;
- payload storage;
- public downloads;
- protected downloads;
- password hashing;
- package hashes;
- initial publication/admin workflow.

Manager implements:

- catalogue browsing;
- package detail;
- public download;
- protected unlock;
- package hash verification.

## Acceptance Criteria

~~~
Public datasets can be browsed without Manager.

Datasets are grouped by guild/player.

Public packages can be downloaded.

Protected package payloads cannot be fetched without valid authorization.

Passwords are not stored plaintext.

Manager can browse the catalogue.

Manager can unlock a protected dataset.

Downloaded hashes are verified.

Invalid hashes prevent installation.
~~~

---

# 74. Phase 4 — Full Dataset Package Management

Implement:

- install;
- update;
- remove;
- dependency resolution;
- local modification detection;
- compatibility checks;
- multiple account targets;
- multiple WoW installations;
- Update All;
- manual-package adoption;
- rpe:// deep links.

## Acceptance Criteria

~~~
A catalogue dataset can be installed with one user action.

A later revision appears as an update.

Updating retains the dataset ID.

Dependencies install before dependants.

Required missing dependencies cannot be silently ignored.

Removing a dependency warns about dependants.

Locally modified datasets are not overwritten silently.

Protected datasets require authorization.

Website Install buttons open the correct Manager package.

Multiple accounts can be independently targeted.
~~~

---

# 75. Phase 5 — RPEngine Installation Management

RPEngine addon binaries are distributed through **GitHub Releases in `FrontierDev/rpe2`**.

Esarus does not duplicate or independently host RPEngine release ZIPs. Its role is to provide the Manager-facing release metadata and channel policy that identifies which GitHub Release should be offered.

The intended distribution flow is:

~~~
RPEngine source/tag
    ↓
GitHub Actions in FrontierDev/rpe2
    ↓
versioned GitHub Release
    ↓
clean RPEngine2 release ZIP asset
    ↓
esarus.net release metadata / channel selection
    ↓
RPEngine Manager
    ↓
verified staged installation/update
~~~

Datasets remain a separate distribution system and continue to be served through the Esarus catalogue/backend.

## RPEngine release pipeline

Modify `FrontierDev/rpe2` to implement:

- a defined release/tagging convention;
- GitHub Actions packaging for Windows-compatible WoW addon distribution;
- creation of a clean `RPEngine2` ZIP containing only files required by the addon;
- GitHub Release publication with the versioned ZIP as a release asset;
- deterministic release asset naming;
- release artifact hashing suitable for verification by the Manager/Esarus metadata;
- validation that the packaged TOC version agrees with the release version;
- release build failure if required addon files are missing or unexpected development files are included.

GitHub Releases are the canonical source of RPEngine binaries.

The Manager must not install directly from a source branch checkout or GitHub repository archive.

## Esarus release metadata

Modify `FrontierDev/esarus` to expose Manager-facing metadata for RPEngine releases.

Esarus metadata should identify at least:

- RPEngine version;
- release/update channel;
- GitHub Release identity;
- GitHub Release asset URL;
- expected release asset hash;
- minimum supported Manager version where required;
- compatibility metadata required for datasets or protocol versions.

Esarus determines which release is currently offered for each supported channel, while the actual binary remains hosted by GitHub Releases.

Changing the advertised release/channel must not require copying the addon binary into Esarus storage.

## Manager implementation

Implement:

- retrieval of RPEngine release/channel metadata from esarus.net;
- comparison of the installed TOC version against the advertised version;
- download of the exact GitHub Release asset identified by trusted Esarus metadata;
- release hash verification before extraction or installation;
- RPEngine installation;
- RPEngine updating;
- installation validation;
- staged extraction and replacement;
- backups of the existing addon installation before replacement;
- rollback/preservation of the existing installation if staging or validation fails;
- compatibility-driven addon updates when a dataset requires a newer RPEngine version.

The Manager must continue to use its existing WoW-installation validation and WoW-running safety checks.

## Update channels

The release metadata model must support explicit channels, for example:

~~~
stable
beta
alpha
~~~

The exact channel names may evolve, but channel selection must be explicit and persisted by the Manager.

A user on one channel must not silently receive releases from another channel.

## Release integrity

Before modifying `Interface/AddOns/RPEngine2`, the Manager must verify that:

- the downloaded file matches the expected hash;
- the archive has the expected RPEngine2 package structure;
- `RPEngine2.toc` exists;
- the packaged addon version matches the advertised release;
- extraction completed successfully in a staging location.

A failed download, hash check, archive validation or extraction must leave the existing addon installation unchanged.

## Acceptance Criteria

~~~
A tagged RPEngine release can produce a clean, versioned RPEngine2 ZIP through GitHub Actions.

The ZIP is attached to a GitHub Release and GitHub Releases are the canonical binary source.

Esarus can advertise the current RPEngine release for a configured channel without hosting a duplicate binary.

Manager can install RPEngine into a valid WoW installation from the advertised GitHub Release asset.

Manager verifies the downloaded release hash before modifying the addon installation.

Manager detects the resulting installed version from RPEngine2.toc.

Manager can update an existing installation to a later advertised release.

A failed download, hash verification, extraction or staged validation does not destroy the previous installation.

A dataset compatibility requirement can request the required RPEngine update.

WoW running prevents installation changes.

Datasets continue to be downloaded from Esarus rather than GitHub Releases.
~~~

---

# 76. Phase 6 — Polish and Optional Features

Potential work:

~~~
Manager self-update
desktop update notifications
automatic update checks
secure remembered protected-package access
publisher web portal
package changelogs
package screenshots
download statistics
trusted publisher indicators
macOS support
Linux support
~~~

These are not requirements for the initial usable system.

---

# 77. Explicit Non-Goals

The initial project does not implement:

~~~
A general WoW addon manager
CurseForge compatibility
automatic WoW launching
Battle.net replacement
remote control of RPE
remote execution of Lua
automatic dataset updates without consent
cloud synchronization of character Profiles
cloud synchronization of Inventory
editing another player's SavedVariables
offline modification of character gameplay state
user accounts for every Esarus website visitor
public unrestricted dataset publishing
DRM
strong protection against an authorised user redistributing a downloaded dataset
~~~

Password protection prevents unauthorised server downloads.

It cannot prevent a legitimately authorised recipient from manually copying data they have received.

---

# 78. Principal Architectural Decisions

**RPEngine Manager is a separate repository and desktop application.**

**The Esarus website remains the public catalogue rather than gaining direct access to local WoW files.**

**RPEngine remains authoritative for dataset import, normalization, activation, dependencies and deletion.**

**The Manager communicates through a deliberately small SavedVariables integration contract.**

**The Manager never directly owns or rewrites RPE's canonical dataset database model.**

**Dataset packages retain the existing RPE_DATASET_V1 format.**

**Catalogue revision is separate from RPE dataset ID.**

**A dataset's ID remains stable across package updates.**

**Public and protected datasets share the same package model.**

**Protected dataset payloads are only delivered after server-side authorization.**

**Passwords are never stored in plaintext.**

**All downloaded content is integrity checked before installation.**

**All SavedVariables writes are backed up and performed safely.**

**World of Warcraft must be closed before any Manager-controlled modifications.**

**The Manager handles multiple WoW installations and accounts explicitly.**

**Manual RPE dataset import/export remains supported.**

**The Manager should ultimately manage RPEngine itself as well as its datasets.**

**Website deep links improve the installation experience but do not become a security boundary.**

**Failures are surfaced explicitly rather than hidden behind fallbacks.**

---

# 79. Intended End-State User Experience

New user:

~~~
Visit esarus.net/rpengine
        |
Download RPEngine Manager
        |
Manager detects WoW
        |
Install RPEngine
        |
Browse datasets
        |
Install Esarus Core
        |
Launch WoW
        |
RPE imports and activates dataset
~~~

Returning user:

~~~
Open RPEngine Manager
        |
3 updates available

RPEngine            2.0.alpha5 → 2.0.alpha6
Esarus Core         revision 14 → 15
Esarus Campaign     revision 7 → 8

[ Update All ]
        |
Manager downloads/verifies packages
        |
Manager backs up affected files
        |
RPE addon updated
        |
Dataset operations queued
        |
Launch WoW
        |
RPE consumes updates canonically
~~~

Website-driven install:

~~~
esarus.net/rpengine/datasets/esarus-campaign
        |
[ Install with RPEngine Manager ]
        |
rpe://dataset/esarus-campaign
        |
Manager opens package
        |
resolve dependencies
        |
[ Install ]
~~~

This provides one coherent installation and update system without coupling the website directly to the user's WoW files or coupling the desktop application to RPE's evolving internal database implementation.
