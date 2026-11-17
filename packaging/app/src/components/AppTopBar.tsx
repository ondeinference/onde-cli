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
    <header
      data-tauri-drag-region
      className="fixed left-0 top-0 z-50 flex w-full items-center justify-between gap-3 border-b border-fog-line bg-paper pr-3"
      style={{ height: "var(--titlebar-height)", paddingLeft: inset }}
    >
      <div className="flex min-w-0 items-center gap-2">
        <button
          type="button"
          onClick={onToggleSidebar}
          aria-label="Toggle sidebar"
          title="Toggle sidebar"
          aria-expanded={sidebarOpen}
          aria-controls="sidebar-panel"
          className="flex shrink-0 cursor-pointer items-center p-1 hover:opacity-60"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" aria-hidden="true">
            <rect x="3" y="4.5" width="18" height="15" rx="3" />
            <line x1="9.5" y1="4.5" x2="9.5" y2="19.5" />
          </svg>
        </button>
        <img src="/favicon.svg" alt="" width={20} height={20} className="pointer-events-none shrink-0 rounded-[4px]" />
        <span className="pointer-events-none truncate text-sm font-semibold">Onde Desktop</span>
      </div>
      <div className="flex shrink-0 items-center gap-2">{actions}</div>
    </header>
  );
}
