import type { PropsWithChildren } from "react";

export type ManagerPage = "manager" | "advanced";

interface AppFrameProps extends PropsWithChildren {
  page: ManagerPage;
  onNavigate: (page: ManagerPage) => void;
}

export function AppFrame({ children, page, onNavigate }: AppFrameProps) {
  return (
    <div className="app-window">
      <header className="topbar">
        <button className="brand" type="button" onClick={() => onNavigate("manager")}>
          <img className="brand-icon" src="/rpe.png" alt="" />
          <span className="brand-name">RPEngine Manager</span>
        </button>
        <nav className="primary-nav" aria-label="Manager navigation">
          {(["manager", "advanced"] as const).map((item) => (
            <button
              className={page === item ? "nav-button nav-button-active" : "nav-button"}
              key={item}
              type="button"
              onClick={() => onNavigate(item)}
            >
              {item[0].toUpperCase() + item.slice(1)}
            </button>
          ))}
        </nav>
      </header>
      <main className="main-content">{children}</main>
    </div>
  );
}
