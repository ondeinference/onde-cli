import { useCallback, useEffect, useState } from "react";
import { ipc } from "@/lib/ipc";
import { errorMessage, type ProjectSummary, type ProjectsLocation } from "@/lib/types";
import { ProjectDetail } from "./ProjectDetail";
import { ProjectList } from "./ProjectList";
import { ProjectsFolder } from "./ProjectsFolder";

export function ProjectsView() {
  const [location, setLocation] = useState<ProjectsLocation | null>(null);
  const [projects, setProjects] = useState<ProjectSummary[] | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setError(null);
    Promise.all([ipc.projectsLocation(), ipc.listProjects()])
      .then(([l, p]) => {
        setLocation(l);
        setProjects(p);
      })
      .catch((e) => setError(errorMessage(e)));
  }, []);

  useEffect(refresh, [refresh]);

  if (open) return <ProjectDetail repoId={open} onBack={() => { setOpen(null); refresh(); }} />;

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-lg font-semibold">Projects</h1>
        <p className="mt-1 text-sm text-fog">Datasets and fine-tune runs, one project per model.</p>
      </div>
      {error && <p className="text-sm text-red-600">{error}</p>}
      {location && (
        <ProjectsFolder
          location={location}
          onChange={(l) => {
            setLocation(l);
            refresh();
          }}
        />
      )}
      {location && projects && <ProjectList projects={projects} location={location} onOpen={setOpen} />}
    </div>
  );
}
