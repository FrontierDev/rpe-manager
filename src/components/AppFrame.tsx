import type { PropsWithChildren } from "react";

export type ManagerPage = "home" | "rpengine" | "settings";

interface AppFrameProps extends PropsWithChildren {
  page: ManagerPage;
  onNavigate: (page: ManagerPage) => void;
}

export function AppFrame({ children, page, onNavigate }: AppFrameProps) {
  return (
    <div className="app-window">
      <header className="topbar">
        <button className="brand" type="button" onClick={() => onNavigate("home")}>
          <span className="brand-mark" aria-hidden="true">RP</span>
          <span className="brand-name"><strong>RPEngine</strong><span>MANAGER</span></span>
        </button>
        <nav className="primary-nav" aria-label="Manager navigation">
          {(["home", "rpengine", "settings"] as const).map((item) => (
            <button
              className={page === item ? "nav-button nav-button-active" : "nav-button"}
              key={item}
              type="button"
              onClick={() => onNavigate(item)}
            >
              {item === "rpengine" ? "RPEngine" : item[0].toUpperCase() + item.slice(1)}
            </button>
          ))}
        </nav>
        <div className="topbar-meta"><span className="environment-dot" aria-hidden="true" /><span>PHASE 1</span></div>
      </header>
      <main className="main-content">{children}</main>
    </div>
  );
}
