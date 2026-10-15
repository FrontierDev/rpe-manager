import { useMemo, useState, type ReactNode } from "react";
import { Panel } from "../../../core/ui/Panel";
import { catalogueErrorMessage } from "../../api/catalogue";
import type { CatalogueFetchState, CataloguePackage } from "../../models/catalogue";
import { catalogueInstallStateLabel, cataloguePackagePresentation } from "../../models/catalogue";
import type { DatasetRow } from "../../models/datasets";
import type { SelectedProtocolState } from "../../models/protocol-state";

interface Props {
  packages: CataloguePackage[];
  localRows: DatasetRow[];
  fetchState: CatalogueFetchState;
  error: string | null;
  protocolState: SelectedProtocolState | null;
  selectedAccounts: string[];
  controls: ReactNode;
  imports: ReactNode;
  onRefresh: () => void;
  onInstall: (pkg: CataloguePackage, password?: string) => Promise<string>;
  onRemove: (row: DatasetRow) => Promise<string>;
}

interface PasswordPrompt {
  pkg: CataloguePackage;
  password: string;
  error: string | null;
}

type FilterColumn = "package" | "publisher" | "type" | "dataset" | null;

export function CataloguePanel({
  packages,
  localRows,
  fetchState,
  error,
  protocolState,
  selectedAccounts,
  controls,
  imports,
  onRefresh,
  onInstall,
  onRemove,
}: Props) {
  const [packageQuery, setPackageQuery] = useState("");
  const [publisherQuery, setPublisherQuery] = useState("");
  const [datasetQuery, setDatasetQuery] = useState("");
  const [type, setType] = useState("all");
  const [activeFilter, setActiveFilter] = useState<FilterColumn>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [removalMessage, setRemovalMessage] = useState<string | null>(null);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [packagePasswords, setPackagePasswords] = useState<Record<string, string>>({});
  const [passwordPrompt, setPasswordPrompt] = useState<PasswordPrompt | null>(null);
  const accounts = protocolState?.accounts ?? [];
  const filtered = useMemo(() => packages.filter((pkg) =>
    (type === "all" || pkg.packageType === type)
    && pkg.name.toLowerCase().includes(packageQuery.toLowerCase())
    && pkg.publisherDisplayName.toLowerCase().includes(publisherQuery.toLowerCase())
    && `${pkg.datasetType ?? ""} ${pkg.datasetGroup ?? ""}`.toLowerCase().includes(datasetQuery.toLowerCase()),
  ), [packages, packageQuery, publisherQuery, datasetQuery, type]);
  const localByCatalogueId = useMemo(
    () => new Map(localRows.filter((row) => row.catalogueId).map((row) => [row.catalogueId!, row])),
    [localRows],
  );
  const localOnly = useMemo(() => localRows
    .filter((row) => !row.catalogueId || !packages.some((pkg) => pkg.catalogueId === row.catalogueId))
    .filter((row) => row.dataset.toLowerCase().includes(packageQuery.toLowerCase())
      && row.publisher.toLowerCase().includes(publisherQuery.toLowerCase())
      && row.category.toLowerCase().includes(datasetQuery.toLowerCase())
      && (type === "all" || type === "dataset")),
  [localRows, packages, packageQuery, publisherQuery, datasetQuery, type]);

  const queueInstall = (pkg: CataloguePackage, password?: string) => {
    setInstallingId(pkg.catalogueId);
    setMessage(null);
    void onInstall(pkg, password)
      .then((text) => {
        if (pkg.protected && password) setPackagePasswords((current) => ({ ...current, [pkg.catalogueId]: password }));
        setPasswordPrompt(null);
        setMessage(text);
      })
      .catch((reason: unknown) => {
        const errorMessage = catalogueErrorMessage(reason, "Catalogue package could not be queued.");
        if (pkg.protected && /(?:invalid_password|missing_password|password was rejected)/i.test(errorMessage)) {
          setPackagePasswords((current) => {
            const next = { ...current };
            delete next[pkg.catalogueId];
            return next;
          });
          setPasswordPrompt({ pkg, password: "", error: "That password was not accepted. Enter it again to continue." });
        } else {
          setMessage(errorMessage);
        }
      })
      .finally(() => setInstallingId(null));
  };
  const beginInstall = (pkg: CataloguePackage) => {
    if (!window.confirm(`Queue ${pkg.name} for ${selectedAccounts.length} selected account(s)?`)) return;
    const password = packagePasswords[pkg.catalogueId];
    if (pkg.protected && !password) {
      setPasswordPrompt({ pkg, password: "", error: null });
      return;
    }
    queueInstall(pkg, password);
  };
  const submitPassword = () => {
    if (!passwordPrompt || !passwordPrompt.password.trim()) return;
    queueInstall(passwordPrompt.pkg, passwordPrompt.password);
  };
  const remove = (row: DatasetRow) => {
    if (!window.confirm(`Queue removal of ${row.dataset}?`)) return;
    setRemovingId(row.id);
    setRemovalMessage(null);
    void onRemove(row)
      .then(setRemovalMessage)
      .catch((reason: unknown) => setRemovalMessage(catalogueErrorMessage(reason, "Dataset removal could not be queued.")))
      .finally(() => setRemovingId(null));
  };
  const showLoadingPanel = fetchState === "loading" && packages.length === 0 && localRows.length === 0;

  return <section className="dataset-section" aria-labelledby="catalogue-heading">
    <div className="section-heading">
      <h2 id="catalogue-heading">Datasets</h2>
      <div className="import-actions">
        {controls}
        <button className="secondary-button" type="button" onClick={onRefresh} disabled={fetchState === "loading"}>
          {fetchState === "loading" ? "Loading…" : "Refresh catalogue"}
        </button>
      </div>
    </div>
    {error ? <p className="discovery-error">{error}</p> : null}
    {imports}
    {showLoadingPanel ? <Panel className="empty-panel"><p>Loading live catalogue…</p></Panel> : <div className="dataset-table-wrap">
      <table className="dataset-table">
        <thead><tr>
          <FilterHeader title="Package" active={activeFilter === "package"} onClick={() => setActiveFilter((current) => current === "package" ? null : "package")}><input autoFocus value={packageQuery} onChange={(event) => setPackageQuery(event.target.value)} placeholder="Search package" /></FilterHeader>
          <FilterHeader title="Publisher" active={activeFilter === "publisher"} onClick={() => setActiveFilter((current) => current === "publisher" ? null : "publisher")}><input autoFocus value={publisherQuery} onChange={(event) => setPublisherQuery(event.target.value)} placeholder="Search publisher" /></FilterHeader>
          <FilterHeader title="Type" active={activeFilter === "type"} onClick={() => setActiveFilter((current) => current === "type" ? null : "type")}><select value={type} onChange={(event) => setType(event.target.value)}><option value="all">All packages</option><option value="dataset">Datasets</option><option value="ruleset">Rulesets</option></select></FilterHeader>
          <FilterHeader title="Dataset type/group" active={activeFilter === "dataset"} onClick={() => setActiveFilter((current) => current === "dataset" ? null : "dataset")}><input autoFocus value={datasetQuery} onChange={(event) => setDatasetQuery(event.target.value)} placeholder="Search type or group" /></FilterHeader>
          <th>Installed</th><th>Available</th><th>State</th><th>Action</th>
        </tr></thead>
        <tbody>
          {filtered.length === 0 && localOnly.length === 0 ? <tr><td colSpan={8}>No datasets match.</td></tr> : <>
            {filtered.map((pkg) => <CatalogueRow key={pkg.catalogueId} pkg={pkg} local={localByCatalogueId.get(pkg.catalogueId)} presentation={cataloguePackagePresentation(pkg, accounts)} onInstall={beginInstall} onRemove={remove} installing={installingId === pkg.catalogueId} removing={removingId === localByCatalogueId.get(pkg.catalogueId)?.id} />)}
            {localOnly.map((row) => <LocalDatasetRow key={row.id} row={row} withdrawn={fetchState === "complete" && isWithdrawnCatalogueRow(row, packages)} onRemove={remove} removing={removingId === row.id} />)}
          </>}
        </tbody>
      </table>
    </div>}
    {message ? <p className={message.startsWith("Queued") ? "status-detail" : "discovery-error"}>{message}</p> : null}
    {removalMessage ? <p className={removalMessage.startsWith("Failed:") ? "discovery-error" : "status-detail"}>{removalMessage}</p> : null}
    {passwordPrompt ? <div className="modal-backdrop" role="presentation"><Panel className="password-dialog" role="dialog" aria-modal="true" aria-labelledby="package-password-title">
      <h3 id="package-password-title">Package password</h3>
      <p>Enter the password for {passwordPrompt.pkg.name}. It will be kept for this Manager session so updates do not need to ask again.</p>
      {passwordPrompt.error ? <p className="discovery-error">{passwordPrompt.error}</p> : null}
      <label>Password <input autoFocus type="password" autoComplete="current-password" value={passwordPrompt.password} onChange={(event) => setPasswordPrompt((current) => current ? { ...current, password: event.target.value, error: null } : null)} onKeyDown={(event) => { if (event.key === "Enter") submitPassword(); }} /></label>
      <div className="dialog-actions"><button className="secondary-button" type="button" disabled={installingId !== null} onClick={() => setPasswordPrompt(null)}>Cancel</button><button className="primary-button" type="button" disabled={!passwordPrompt.password.trim() || installingId !== null} onClick={submitPassword}>{installingId ? "Queueing…" : "Continue"}</button></div>
    </Panel></div> : null}
  </section>;
}

function FilterHeader({ title, active, onClick, children }: { title: string; active: boolean; onClick: () => void; children: ReactNode }) {
  return <th className={`filter-header${active ? " filter-header-active" : ""}`}><button type="button" onClick={onClick} aria-expanded={active}>{title}</button>{active ? <div className="header-filter-control">{children}</div> : null}</th>;
}

function CatalogueRow({ pkg, local, presentation, onInstall, onRemove, installing, removing }: { pkg: CataloguePackage; local?: DatasetRow; presentation: ReturnType<typeof cataloguePackagePresentation>; onInstall: (pkg: CataloguePackage) => void; onRemove: (row: DatasetRow) => void; installing: boolean; removing: boolean }) {
  const removalRow = local && presentation.installedPackage ? { ...local, catalogueId: presentation.installedPackage.catalogueId, datasetId: presentation.installedPackage.datasetId, installedRevision: presentation.installedPackage.revision, hash: presentation.installedPackage.hash, status: "installed" as const } : null;
  const isInstall = presentation.installedPackage === null;
  const actionAvailable = pkg.packageType === "dataset" && presentation.state !== "queued" && (isInstall || presentation.hasUpdate);
  const label = catalogueInstallStateLabel(presentation.state);
  return <tr>
    <td>{pkg.name}{pkg.protected ? " 🔒" : ""}</td><td>{pkg.publisherDisplayName}</td><td>{pkg.packageType}</td><td>{[pkg.datasetType, pkg.datasetGroup].filter(Boolean).join(" / ") || "—"}</td>
    <td>{presentation.installedRevision ?? "—"}</td><td>{presentation.availableRevision}</td>
    <td><span className={`dataset-status dataset-status-${presentation.state}`} title={presentation.state === "queued" ? "Start or reload RPEngine in World of Warcraft to process this request." : undefined}>{label}</span></td>
    <td className="catalogue-actions"><button className="catalogue-action catalogue-update-button" type="button" disabled={!actionAvailable || installing} onClick={() => onInstall(pkg)}>{installing ? "Queueing…" : isInstall ? "Install" : "Update"}</button><button className="catalogue-action catalogue-remove-button" type="button" disabled={!removalRow || removing} onClick={() => { if (removalRow) onRemove(removalRow); }}>{removing ? "Queueing…" : "Remove"}</button></td>
  </tr>;
}

function LocalDatasetRow({ row, withdrawn, onRemove, removing }: { row: DatasetRow; withdrawn: boolean; onRemove: (row: DatasetRow) => void; removing: boolean }) {
  const hasInstalledIdentity = row.installedRevision !== null && row.hash !== null;
  const canRemove = row.status === "installed" || (withdrawn && hasInstalledIdentity);
  const isPending = row.status === "pending";
  const withdrawnInstalled = withdrawn && hasInstalledIdentity;
  const status = withdrawnInstalled ? "Installed — no longer listed" : isPending ? "Login Required" : row.status.replaceAll("_", " ");
  return <tr>
    <td>{row.dataset}</td><td>{row.publisher}</td><td>dataset</td><td>{row.category}</td><td>{row.installedRevision ?? "—"}</td><td>—</td>
    <td><span className={`dataset-status dataset-status-${withdrawnInstalled ? "no-longer-listed" : row.status}`} title={isPending ? "Start or reload RPEngine in World of Warcraft to process this request." : undefined}>{status}</span>{withdrawn ? <small className="status-detail">Catalogue ID: {row.catalogueId}</small> : null}{withdrawnInstalled && isPending ? <small className="status-detail">Pending operation: Login Required until RPE loads.</small> : null}</td>
    <td>{canRemove ? <button className="link-button" type="button" disabled={removing} onClick={() => onRemove(row)}>{removing ? "Queueing…" : "Remove"}</button> : <span className="status-detail">{isPending ? "Start or reload RPEngine to process this request." : "Managed locally"}</span>}</td>
  </tr>;
}

function isWithdrawnCatalogueRow(row: DatasetRow, packages: CataloguePackage[]): boolean {
  return (row.status === "installed" || (row.status === "pending" && row.installedRevision !== null && row.hash !== null))
    && row.catalogueId !== null
    && row.catalogueId !== "local"
    && !row.catalogueId.startsWith("manual.")
    && !packages.some((pkg) => pkg.catalogueId === row.catalogueId);
}
