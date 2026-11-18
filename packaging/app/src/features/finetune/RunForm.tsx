import { useEffect, useState } from "react";
import { ipc } from "@/lib/ipc";
import { errorMessage, type JobKind, type JobRequest, type ProjectSummary } from "@/lib/types";

const DTYPES = [
  { id: "q4_k_m", label: "Q4_K_M (smallest, the usual choice)" },
  { id: "q8_0", label: "Q8_0 (larger, closer to the original)" },
  { id: "f16", label: "F16 (largest)" },
] as const;

/** Why a project cannot start a job, or null when it can. */
function blocked(p: ProjectSummary): string | null {
  if (!p.base_ready) return "its base model is not downloaded (do that on the project page)";
  if (p.dataset === "missing") return "it has no dataset yet";
  if (p.dataset === "invalid") return "its dataset has errors";
  return null;
}

/** "1e-4" and "0.0001" both work. Anything else is NaN. */
const num = (text: string) => (text.trim() === "" ? NaN : Number(text));

export function RunForm({
  disabled,
  onStart,
}: {
  /** A job is running, so a second cannot start. */
  disabled: boolean;
  onStart: (request: JobRequest, datasetRows: number | null) => Promise<void>;
}) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [repoId, setRepoId] = useState("");
  const [kind, setKind] = useState<JobKind>("census");
  const [name, setName] = useState("");
  const [lr, setLr] = useState("1e-4");
  const [epochs, setEpochs] = useState("2");
  const [rank, setRank] = useState("8");
  const [alpha, setAlpha] = useState("16");
  const [maxSeq, setMaxSeq] = useState("512");
  const [dtype, setDtype] = useState<JobRequest["dtype"]>("q4_k_m");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc
      .listProjects()
      .then((list) => {
        setProjects(list);
        const ready = list.find((p) => blocked(p) === null);
        setRepoId((current) => current || ready?.repo_id || "");
      })
      .catch((e) => setError(errorMessage(e)));
  }, []);

  const project = projects.find((p) => p.repo_id === repoId);
  const why = project ? blocked(project) : "there are no projects yet";
  const census = kind === "census";

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await onStart(
        {
          repo_id: repoId,
          kind,
          name: census ? "" : name.trim(),
          learning_rate: num(lr),
          epochs: num(epochs),
          lora_rank: num(rank),
          lora_alpha: num(alpha),
          max_seq_len: num(maxSeq),
          dtype,
        },
        project?.dataset_rows ?? null,
      );
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  const field = "w-full rounded-md border border-fog-line bg-transparent px-3 py-1.5 text-sm outline-none focus:border-ink disabled:opacity-50";
  const label = "flex flex-col gap-1 text-xs text-fog";
  const numbers = !census
    ? Number.isFinite(num(lr)) && Number.isFinite(num(epochs))
    : true;
  const ready =
    !disabled &&
    why === null &&
    numbers &&
    Number.isFinite(num(rank)) &&
    Number.isFinite(num(alpha)) &&
    Number.isFinite(num(maxSeq));

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-lg border border-fog-line px-4 py-3">
      <label className={label}>
        Project
        <select className={field} value={repoId} onChange={(e) => setRepoId(e.target.value)} disabled={disabled}>
          {projects.length === 0 && <option value="">No projects</option>}
          {projects.map((p) => (
            <option key={p.repo_id} value={p.repo_id}>
              {p.repo_id}
              {blocked(p) ? " (not ready)" : ""}
            </option>
          ))}
        </select>
      </label>
      {why && repoId && <p className="text-xs text-amber-600">Can't start: {why}.</p>}

      <fieldset className="flex flex-col gap-2" disabled={disabled}>
        <legend className="mb-1 text-xs text-fog">What to run</legend>
        <label className="flex cursor-pointer items-start gap-2 text-sm">
          <input type="radio" name="kind" checked={census} onChange={() => setKind("census")} className="mt-1" />
          <span>
            Gradient census
            <span className="block text-xs text-fog">
              One pass at a learning rate of 0. It trains nothing and checks that gradients are sane. Run it before you spend an hour training.
            </span>
          </span>
        </label>
        <label className="flex cursor-pointer items-start gap-2 text-sm">
          <input type="radio" name="kind" checked={!census} onChange={() => setKind("run")} className="mt-1" />
          <span>
            Training run
            <span className="block text-xs text-fog">Train a LoRA adapter, merge it into the base model and export a GGUF.</span>
          </span>
        </label>
      </fieldset>

      {!census && (
        <div className="grid grid-cols-2 gap-3">
          <label className={label}>
            Run name
            <input className={field} value={name} onChange={(e) => setName(e.target.value)} placeholder="e2-lr1e-4" disabled={disabled} spellCheck={false} />
          </label>
          <label className={label}>
            Export as
            <select className={field} value={dtype} onChange={(e) => setDtype(e.target.value as JobRequest["dtype"])} disabled={disabled}>
              {DTYPES.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.label}
                </option>
              ))}
            </select>
          </label>
          <label className={label}>
            Learning rate
            <input className={field} value={lr} onChange={(e) => setLr(e.target.value)} inputMode="decimal" disabled={disabled} />
          </label>
          <label className={label}>
            Epochs
            <input className={field} value={epochs} onChange={(e) => setEpochs(e.target.value)} inputMode="numeric" disabled={disabled} />
          </label>
        </div>
      )}
      <details className="text-xs text-fog">
        <summary className="cursor-pointer">LoRA settings</summary>
        <div className="mt-2 grid grid-cols-3 gap-3">
          <label className={label}>
            Rank
            <input className={field} value={rank} onChange={(e) => setRank(e.target.value)} inputMode="numeric" disabled={disabled} />
          </label>
          <label className={label}>
            Alpha
            <input className={field} value={alpha} onChange={(e) => setAlpha(e.target.value)} inputMode="decimal" disabled={disabled} />
          </label>
          <label className={label}>
            Max sequence
            <input className={field} value={maxSeq} onChange={(e) => setMaxSeq(e.target.value)} inputMode="numeric" disabled={disabled} />
          </label>
        </div>
      </details>

      {error && (
        <p role="alert" className="text-xs text-red-600">
          {error}
        </p>
      )}
      <div className="flex items-center justify-end gap-3">
        {disabled && <span className="text-xs text-fog">A job is running.</span>}
        <button
          type="submit"
          disabled={!ready || busy}
          className="cursor-pointer rounded-md bg-ink px-3 py-1.5 text-xs font-medium text-paper disabled:cursor-default disabled:opacity-50"
        >
          {busy ? "Starting…" : census ? "Run census" : "Start training"}
        </button>
      </div>
    </form>
  );
}
