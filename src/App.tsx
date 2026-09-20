import { Panel } from "../core/ui/Panel";

const foundationItems = [
  {
    label: "Desktop application",
    value: "Ready",
    detail: "The app shell is running locally.",
    status: "ready",
  },
  {
    label: "World of Warcraft",
    value: "Not configured",
    detail: "Installation discovery is part of a later phase.",
    status: "pending",
  },
  {
    label: "Dataset catalogue",
    value: "Not connected",
    detail: "No remote services are used by this scaffold.",
    status: "pending",
  },
] as const;

function App() {
  return (
    <div className="app-window">
      <header className="topbar">
        <a className="brand" href="#overview" aria-label="RPEngine Manager home">
          <span className="brand-mark" aria-hidden="true">
            RP
          </span>
          <span className="brand-name">
            <strong>RPEngine</strong>
            <span>MANAGER</span>
          </span>
        </a>
        <div className="topbar-meta">
          <span className="environment-dot" aria-hidden="true" />
          <span>Desktop foundation</span>
        </div>
      </header>

      <main id="overview" className="main-content">
        <div className="eyebrow">
          <span className="eyebrow-line" />
          YOUR LOCAL CONTROL CENTRE
        </div>
        <section className="welcome-section" aria-labelledby="welcome-heading">
          <div className="welcome-copy">
            <p className="welcome-kicker">Welcome to</p>
            <h1 id="welcome-heading">
              RPEngine <span>Manager</span>
            </h1>
            <p className="welcome-description">
              A home for your RPEngine setup. This desktop foundation is ready;
              local installation discovery and package management will arrive in
              later phases.
            </p>
          </div>
          <div className="welcome-orbit" aria-hidden="true">
            <div className="orbit orbit-outer" />
            <div className="orbit orbit-inner" />
            <div className="orbit-core">RP</div>
            <span className="orbit-node orbit-node-top" />
            <span className="orbit-node orbit-node-right" />
            <span className="orbit-node orbit-node-bottom" />
          </div>
        </section>

        <section className="foundation-section" aria-labelledby="foundation-heading">
          <div className="section-heading">
            <div>
              <p className="section-overline">SYSTEM OVERVIEW</p>
              <h2 id="foundation-heading">Foundation status</h2>
            </div>
            <span className="phase-badge">PHASE 1 · SCAFFOLD</span>
          </div>

          <div className="foundation-grid">
            {foundationItems.map((item) => (
              <Panel className="status-card" key={item.label}>
                <div className={`status-indicator status-${item.status}`} />
                <p className="status-label">{item.label}</p>
                <h3>{item.value}</h3>
                <p className="status-detail">{item.detail}</p>
              </Panel>
            ))}
          </div>
        </section>

        <footer className="app-footer">
          <span>RPEngine Manager</span>
          <span className="footer-separator" aria-hidden="true">
            /
          </span>
          <span>Windows desktop</span>
          <span className="footer-build">LOCAL BUILD</span>
        </footer>
      </main>
    </div>
  );
}

export default App;
