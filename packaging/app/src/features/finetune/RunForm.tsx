import { useEffect, useState } from "react";
import { ipc } from "@/lib/ipc";
import { errorMessage, type JobRequest, type ProjectSummary } from "@/lib/types";
import { loraProblem, runProblem } from "./settings";
import { MAX_SWEEP_RUNS, parseEpochs, parseRates, planSweep, type SweepPlan } from "./sweep";

/** What the form can start. A sweep is several training runs. */
type Mode = "census" | "run" | "sweep";

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
  onSweep,
}: {
  /** A job is running, so a second cannot start. */
  disabled: boolean;
  onStart: (request: JobRequest, datasetRows: number | null) => Promise<void>;
  onSweep: (plan: SweepPlan, datasetRows: number | null) => Promise<void>;
}) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [repoId, setRepoId] = useState("");
  const [kind, setKind] = useState<Mode>("census");
  const [sweepEpochs, setSweepEpochs] = useState("2, 3");
  const [sweepRates, setSweepRates] = useState("5e-5, 1e-4");
  // Names of the runs the project already has, so a sweep does not repeat them.
  // Tagged with its project: another project's names must never be used, and
  // until a project's names have loaded (or failed to) they are unknown, which
  // is not the same as "none".
  const [existingFor, setExistingFor] = useState<{ repoId: string; names: string[] } | null>(null);
  const [existingError, setExistingError] = useState<string | null>(null);
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

  useEffect(() => {
    if (!repoId) return;
    let current = true;
    ipc
      .projectDetail(repoId)
      .then((d) => {
        if (!current) return;
        setExistingFor({ repoId, names: d.runs.map((r) => r.name) });
        setExistingError(null);
      })
      .catch((e) => {
        if (!current) return;
        setExistingFor(null);
        setExistingError(errorMessage(e));
      });
    return () => {
      current = false;
    };
  }, [repoId, disabled]);
  const existing = existingFor?.repoId === repoId ? existingFor.names : null;

  const project = projects.find((p) => p.repo_id === repoId);
  const why = project ? blocked(project) : "there are no projects yet";
  const census = kind === "census";
  const sweeping = kind === "sweep";

  const epochList = parseEpochs(sweepEpochs);
  const rateList = parseRates(sweepRates);
  const base: JobRequest = {
    repo_id: repoId,
    kind: "run",
    name: "",
    learning_rate: 1e-4,
    epochs: 1,
    lora_rank: num(rank),
    lora_alpha: num(alpha),
    max_seq_len: num(maxSeq),
    dtype,
  };
  const plan =
    epochList.ok && rateList.ok && existing !== null ? planSweep(base, epochList.values, rateList.values, existing) : null;
  const sweepProblem = !sweeping
    ? null
    : !epochList.ok
      ? epochList.error
      : !rateList.ok
        ? rateList.error
        : existing === null
          ? existingError
            ? `Could not read this project's runs (${existingError}), so the runs that already exist are not known.`
            : "Reading the project's runs…"
          : plan && plan.runs.length === 0
          ? "Every run in this grid already exists."
          : plan && plan.runs.length > MAX_SWEEP_RUNS
            ? `That is ${plan.runs.length} runs. The most one sweep can hold is ${MAX_SWEEP_RUNS}.`
            : null;

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      if (sweeping) {
        if (plan && !sweepProblem) await onSweep(plan, project?.dataset_rows ?? null);
        return;
      }
      await onStart(
        {
          repo_id: repoId,
          kind: census ? "census" : "run",
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
  // Every reason Start can be disabled is shown, because several of the fields
  // are out of sight: the LoRA settings sit in a disclosure that starts closed.
  const loraIssue = loraProblem(rank, alpha, maxSeq);
  const runIssue = kind === "run" ? runProblem(lr, epochs) : null;
  const ready = !disabled && why === null && !loraIssue && !runIssue && !sweepProblem;

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
          <input type="radio" name="kind" checked={kind === "run"} onChange={() => setKind("run")} className="mt-1" />
          <span>
            Training run
            <span className="block text-xs text-fog">Train a LoRA adapter, merge it into the base model and export a GGUF.</span>
          </span>
        </label>
        <label className="flex cursor-pointer items-start gap-2 text-sm">
          <input type="radio" name="kind" checked={sweeping} onChange={() => setKind("sweep")} className="mt-1" />
          <span>
            Sweep
            <span className="block text-xs text-fog">
              Train once for each epoch count and learning rate you list, one after another, then compare them under Projects.
            </span>
          </span>
        </label>
      </fieldset>

      {sweeping && (
        <div className="grid grid-cols-2 gap-3">
          <label className={label}>
            Epochs
            <input className={field} value={sweepEpochs} onChange={(e) => setSweepEpochs(e.target.value)} disabled={disabled} spellCheck={false} />
          </label>
          <label className={label}>
            Learning rates
            <input className={field} value={sweepRates} onChange={(e) => setSweepRates(e.target.value)} disabled={disabled} spellCheck={false} />
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
          <div className="col-span-2 text-xs text-fog" aria-live="polite">
            {sweepProblem ? (
              <p className="text-amber-600">{sweepProblem}</p>
            ) : plan ? (
              <>
                <p>
                  {plan.runs.length} {plan.runs.length === 1 ? "run" : "runs"}: {plan.runs.map((r) => r.name).join(", ")}
                </p>
                {plan.skipped.length > 0 && <p>Already in the project, so left alone: {plan.skipped.join(", ")}</p>}
                <p>
                  Each run uses LoRA rank {rank.trim()}, alpha {alpha.trim()} and max sequence {maxSeq.trim()}. Change them under LoRA
                  settings.
                </p>
                <p>Each run trains, merges and exports a GGUF of its own, so a sweep takes as long as all of them together.</p>
              </>
            ) : null}
          </div>
        </div>
      )}

      {kind === "run" && (
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
      <details className="text-xs text-fog" open={loraIssue !== null || undefined}>
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
      {loraIssue && <p className="text-xs text-amber-600">{loraIssue}</p>}
      {runIssue && <p className="text-xs text-amber-600">{runIssue}</p>}

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
          {busy ? "Starting…" : census ? "Run census" : sweeping ? (plan ? `Start ${plan.runs.length}-run sweep` : "Start sweep") : "Start training"}
        </button>
      </div>
    </form>
  );
}
