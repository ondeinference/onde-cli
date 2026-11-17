import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isMac } from "@/lib/os";

export function AppTopBar({
  sidebarOpen,
  onToggleSidebar,
  actions,
}: {
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  actions?: ReactNode;
}) {
  const [isFullscreen, setIsFullscreen] = useState(false);

  useEffect(() => {
    if (!isMac) return;
    try {
      const w = getCurrentWindow();
      const sync = () => w.isFullscreen().then(setIsFullscreen).catch(() => {});
      sync();
      const unlisten = w.onResized(sync);
      return () => {
        unlisten.then((fn) => fn());
      };
    } catch {
      // Not running inside Tauri.
    }
  }, []);

  // macOS hides the traffic lights in fullscreen, so the 96px inset goes too.
  const inset = isMac && !isFullscreen ? 96 : 12;

  return (
    // "deep" makes every child a drag surface too, so the window moves from
    // anywhere along the bar. Buttons still click: Tauri's drag handler stops
    // at a clickable element. Double-click zooms, as in any Mac window.
    <header
      data-tauri-drag-region="deep"
      className="app-chrome fixed left-0 top-0 z-50 flex w-full items-center justify-between gap-3 border-b border-fog-line bg-paper pr-3"
      style={{ height: "var(--titlebar-height)", paddingLeft: inset }}
    >
      {/* macOS hides the window title (hiddenTitle), so the name lives in the
          sidebar; this keeps a heading for assistive tech. */}
      <h1 className="sr-only">Onde Desktop</h1>
      <div className="flex min-w-0 items-center gap-2">
        <button
          type="button"
          onClick={onToggleSidebar}
          aria-label="Toggle sidebar"
          title="Toggle sidebar"
          aria-expanded={sidebarOpen}
          aria-controls="sidebar-panel"
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-fog transition-colors hover:bg-fog-line hover:text-ink"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" aria-hidden="true">
            <rect x="3" y="4.5" width="18" height="15" rx="3" />
            <line x1="9.5" y1="4.5" x2="9.5" y2="19.5" />
          </svg>
        </button>
      </div>
      <div className="flex shrink-0 items-center gap-2">{actions}</div>
    </header>
  );
}
