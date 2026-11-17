import { useState } from "react";
import { ipc } from "@/lib/ipc";
import { errorMessage, isQuietError, type ProjectsLocation } from "@/lib/types";
import { shortPath } from "./format";

const LABEL: Record<ProjectsLocation["kind"], string> = {
  native: "Projects folder",
  container: "Projects folder (inside the app)",
  linked: "Linked projects folder",
};

/** Where the projects come from, and the control to link `~/.onde`. Linking
 *  only matters inside the sandbox, so an unsandboxed run shows the path only. */
export function ProjectsFolder({
  location,
  onChange,
}: {
  location: ProjectsLocation;
  onChange: (location: ProjectsLocation) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function run(action: () => Promise<ProjectsLocation>) {
    setBusy(true);
    setError(null);
    try {
      onChange(await action());
    } catch (e) {
      if (!isQuietError(e)) setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const button =
    "cursor-pointer rounded-md border border-fog-line px-2.5 py-1 text-xs font-medium hover:bg-fog-line disabled:opacity-50";

  return (
    <div className="rounded-lg border border-fog-line px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="text-xs text-fog">{LABEL[location.kind]}</p>
          <p className="truncate font-mono text-xs" title={location.path}>
            {shortPath(location.path)}
          </p>
        </div>
        {location.kind === "container" && (
          <button type="button" className={button} disabled={busy} onClick={() => void run(ipc.linkProjectsFolder)}>
            Link ~/.onde…
          </button>
        )}
        {location.kind === "linked" && (
          <button type="button" className={button} disabled={busy} onClick={() => void run(ipc.unlinkProjectsFolder)}>
            Unlink
          </button>
        )}
      </div>
      {location.kind === "container" && (
        <p className="mt-2 text-xs text-fog">
          Link the .onde folder the onde CLI uses, and the app works on the same projects in place. Nothing is copied.
        </p>
      )}
      {location.note && <p className="mt-2 text-xs text-amber-600">{location.note}</p>}
      {error && (
        <p role="alert" className="mt-2 text-xs text-red-600">
          {error}
        </p>
      )}
    </div>
  );
}
