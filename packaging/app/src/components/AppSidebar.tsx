import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { ipc } from "@/lib/ipc";
import { NAV_ITEMS, type TabId } from "@/components/nav-items";

// "v0.6.0 (20261117.1203)", "v0.6.0 (dev)" for an unbundled run, and nothing
// in a plain browser where there is no app to ask.
function useVersionLabel() {
  const [label, setLabel] = useState("");
  useEffect(() => {
    Promise.all([getVersion(), ipc.appBuild()])
      .then(([version, build]) => {
        if (version) setLabel(`v${version} (${build ?? "dev"})`);
      })
      .catch(() => {});
  }, []);
  return label;
}

export function AppSidebar({
  open,
  active,
  onSelect,
}: {
  open: boolean;
  active: TabId;
  onSelect: (tab: TabId) => void;
}) {
  const versionLabel = useVersionLabel();

  return (
    <nav
      id="sidebar-panel"
      aria-label="Main"
      inert={!open}
      className={`app-chrome fixed bottom-0 left-0 z-40 overflow-hidden border-fog-line bg-paper transition-[width] duration-200 ${open ? "border-r" : "border-r-0"}`}
      style={{ top: "var(--titlebar-height)", width: open ? "var(--sidebar-width)" : 0 }}
    >
      <div className="flex h-full flex-col" style={{ width: "var(--sidebar-width)" }}>
        <div className="flex items-center gap-2 px-4 pt-3 pb-1">
          <img src="/favicon.svg" alt="" width={18} height={18} className="shrink-0 rounded-[4px]" />
          <span className="truncate text-sm font-semibold">Onde Desktop</span>
        </div>
        <ul className="app-scroll flex min-h-0 flex-1 flex-col gap-0.5 p-2">
          {NAV_ITEMS.map((item) => (
            <li key={item.id}>
              <button
                type="button"
                onClick={() => onSelect(item.id)}
                aria-current={active === item.id ? "page" : undefined}
                className="w-full rounded-md px-3 py-1.5 text-left text-sm hover:bg-fog-line aria-[current=page]:bg-fog-line aria-[current=page]:font-medium"
              >
                {item.label}
              </button>
            </li>
          ))}
        </ul>
        {versionLabel && (
          <p className="mt-auto truncate border-t border-fog-line px-4 py-3 font-mono text-[10px] uppercase tracking-wider tabular-nums text-fog">
            {versionLabel}
          </p>
        )}
      </div>
    </nav>
  );
}
