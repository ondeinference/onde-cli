import { useEffect, useState } from "react";
import { ipc } from "@/lib/ipc";
import { errorMessage, type BaseModelView } from "@/lib/types";

// The same shape the backend accepts: org/name, letters, digits, . - and _,
// and neither half may be "." or "..".
const REPO_ID = /^(?!\.{1,2}\/)(?!.*\/\.{1,2}$)[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/;

export function NewProjectForm({
  onCreated,
  onCancel,
}: {
  onCreated: (repoId: string) => void;
  onCancel: () => void;
}) {
  const [models, setModels] = useState<BaseModelView[]>([]);
  const [repoId, setRepoId] = useState("");
  const [base, setBase] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc
      .baseModels()
      .then((m) => {
        setModels(m);
        setBase((current) => current || m[0]?.repo_id || "");
      })
      .catch((e) => setError(errorMessage(e)));
  }, []);

  const valid = REPO_ID.test(repoId.trim()) && base !== "";

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const made = await ipc.createProject(repoId.trim(), base);
      onCreated(made.repo_id);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  const field = "w-full rounded-md border border-fog-line bg-transparent px-3 py-2 text-sm outline-none focus:border-ink";
  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-lg border border-fog-line px-4 py-3">
      <h2 className="text-sm font-medium">New project</h2>
      <label className="flex flex-col gap-1 text-xs text-fog">
        Where it will be published
        <input
          className={field}
          placeholder="organization/model-name"
          value={repoId}
          onChange={(e) => setRepoId(e.target.value)}
          autoFocus
          spellCheck={false}
          autoCapitalize="off"
        />
      </label>
      <label className="flex flex-col gap-1 text-xs text-fog">
        Base model
        <select className={field} value={base} onChange={(e) => setBase(e.target.value)}>
          {models.map((m) => (
            <option key={m.repo_id} value={m.repo_id}>
              {m.display_name} · {m.size_display} · {m.dir ? "downloaded" : "not downloaded"}
            </option>
          ))}
        </select>
      </label>
      {base && <p className="text-xs text-fog">{models.find((m) => m.repo_id === base)?.description}</p>}
      {error && (
        <p role="alert" className="text-xs text-red-600">
          {error}
        </p>
      )}
      <div className="flex items-center justify-end gap-2">
        <button type="button" onClick={onCancel} className="cursor-pointer rounded-md px-3 py-1.5 text-xs hover:bg-fog-line">
          Cancel
        </button>
        <button
          type="submit"
          disabled={!valid || busy}
          className="cursor-pointer rounded-md bg-ink px-3 py-1.5 text-xs font-medium text-paper disabled:opacity-50"
        >
          {busy ? "Creating…" : "Create project"}
        </button>
      </div>
    </form>
  );
}
