import type { PropsWithChildren } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ManagerUpdateBanner } from "./ManagerUpdateStatus";

export type ManagerPage = "manager" | "advanced";

interface AppFrameProps extends PropsWithChildren {
  page: ManagerPage;
  onNavigate: (page: ManagerPage) => void;
}

export function AppFrame({ children, page, onNavigate }: AppFrameProps) {
  const appWindow = getCurrentWindow();
  return (
    <div className="app-window">
      <header className="topbar">
        <div className="window-drag-region" aria-hidden="true" onMouseDown={(event) => { if (event.button === 0) void appWindow.startDragging(); }} />
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
        <div className="window-controls" aria-label="Window controls">
          <button className="window-control" type="button" aria-label="Minimize window" onClick={(event) => { event.stopPropagation(); void appWindow.minimize(); }}><span aria-hidden="true">−</span></button>
          <button className="window-control window-close" type="button" aria-label="Close window" onClick={(event) => { event.stopPropagation(); void appWindow.close(); }}><span aria-hidden="true">×</span></button>
        </div>
      </header>
      <ManagerUpdateBanner />
      <main className="main-content">{children}</main>
    </div>
  );
}
