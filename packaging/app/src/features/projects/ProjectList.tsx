import type { ProjectSummary, ProjectsLocation } from "@/lib/types";
import { formatDate } from "./format";

export function ProjectList({
  projects,
  location,
  onOpen,
}: {
  projects: ProjectSummary[];
  location: ProjectsLocation;
  onOpen: (repoId: string) => void;
}) {
  if (projects.length === 0) {
    return (
      <div className="rounded-lg border border-dashed border-fog-line px-4 py-8 text-center text-sm text-fog">
        <p>No projects yet.</p>
        <p className="mt-1 text-xs">
          {location.kind === "container"
            ? "Link your ~/.onde folder above to see the projects you made with the onde CLI."
            : "Projects you create with the onde CLI show up here."}
        </p>
      </div>
    );
  }

  return (
    <ul className="divide-y divide-fog-line rounded-lg border border-fog-line">
      {projects.map((p) => (
        <li key={p.repo_id}>
          <button
            type="button"
            onClick={() => onOpen(p.repo_id)}
            className="flex w-full cursor-pointer items-center justify-between gap-4 px-4 py-3 text-left hover:bg-fog-line/40"
          >
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{p.repo_id}</p>
              <p className="truncate text-xs text-fog">
                {p.base_model_id}
                {p.created_at ? ` · ${formatDate(p.created_at)}` : ""}
              </p>
            </div>
            <div className="shrink-0 text-right text-xs tabular-nums text-fog">
              <p>{p.dataset_rows == null ? "No dataset" : `${p.dataset_rows} rows`}</p>
              <p>
                {p.runs === 1 ? "1 run" : `${p.runs} runs`}
                {p.latest_run ? ` · ${p.latest_run}` : ""}
              </p>
            </div>
          </button>
        </li>
      ))}
    </ul>
  );
}
