import { LossChart } from "@/features/projects/LossChart";
import { formatBytes } from "@/features/projects/format";
import { censusVerdict } from "./census";
import { formatDuration, isRunning, rowsLeftOut, secondsLeft, stepsDone } from "./job";
import { headline as sweepHeadline, isFinished, lowestLoss } from "./sweep";
import type { Job } from "./useJob";

const BACKEND_NAME = { mlx: "MLX", cpu: "CPU", metal: "Metal" } as const;
const STAGES = [
  { id: "training", label: "Train" },
  { id: "merging", label: "Merge" },
  { id: "exporting", label: "Export" },
] as const;

function Bar({ value, label }: { value: number | undefined; label: string }) {
  return (
    <div className="mt-2">
      <progress className="h-1.5 w-full" max={1} value={value} aria-label={label} />
    </div>
  );
}

export function JobPanel({ job }: { job: Job }) {
  const { state, steps } = job;
  const { info, position, outcome } = state;
  if (!info) return null;

  const running = isRunning(state);
  const sweep = job.sweep;
  // Between two runs of a sweep the last one's outcome is on screen for an
  // instant. Dismissing then would throw the sweep away, so there is no button.
  const sweepActive = sweep !== null && !isFinished(sweep);
  const census = info.kind === "census";
  const total = position ? position.totalEpochs * position.totalSteps : 0;
  const elapsed = job.trainingStartedAt ? Date.now() - job.trainingStartedAt : 0;
  const left = position ? secondsLeft(position, elapsed) : null;
  const skipped = steps.filter((s) => s.skipped != null).length;
  const last = steps[steps.length - 1];
  const verdict = census ? censusVerdict(steps) : null;
  const leftOut = rowsLeftOut(job.datasetRows, position);

  return (
    <section className="rounded-lg border border-fog-line px-4 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="truncate text-sm font-medium">{info.run}</h2>
          <p className="truncate text-xs text-fog">
            {census ? "Gradient census" : sweep ? sweepHeadline(sweep) : "Training run"} ·{" "}
            {info.repo_id}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {state.backend && (
            <span
              className="rounded-md border border-fog-line px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider"
              title={state.backend.fallback ?? undefined}
            >
              {BACKEND_NAME[state.backend.name]}
            </span>
          )}
          {running && sweepActive && (
            <>
              <button
                type="button"
                disabled={state.stopping}
                onClick={job.skipRun}
                className="cursor-pointer rounded-md border border-fog-line px-2.5 py-1 text-xs font-medium hover:bg-fog-line disabled:cursor-default disabled:opacity-60"
              >
                Skip run
              </button>
              <button
                type="button"
                disabled={state.stopping}
                onClick={job.stopSweep}
                className="cursor-pointer rounded-md border border-fog-line px-2.5 py-1 text-xs font-medium hover:bg-fog-line disabled:cursor-default disabled:opacity-60"
              >
                {state.stopping && sweep?.stopped ? "Stopping…" : "Stop sweep"}
              </button>
            </>
          )}
          {running && !sweepActive && (
            <button
              type="button"
              disabled={state.stopping}
              onClick={job.cancel}
              className="cursor-pointer rounded-md border border-fog-line px-2.5 py-1 text-xs font-medium hover:bg-fog-line disabled:cursor-default disabled:opacity-60"
            >
              {state.stopping ? "Stopping…" : "Cancel"}
            </button>
          )}
        </div>
      </div>

      {state.backend?.fallback && (
        <p className="mt-2 text-xs text-amber-600">Using the CPU, which is much slower. {state.backend.fallback}</p>
      )}

      {!census && (
        <ol className="mt-3 flex gap-1 text-xs" aria-label="Stages">
          {STAGES.map((s) => (
            <li
              key={s.id}
              aria-current={state.stage === s.id && running ? "step" : undefined}
              className={`rounded-md px-2 py-0.5 ${state.stage === s.id && running ? "bg-ink text-paper" : "text-fog"}`}
            >
              {s.label}
            </li>
          ))}
        </ol>
      )}

      {running && state.stopping && (
        <p className="mt-2 text-xs text-fog">
          {state.stage === "training" || state.stage === null
            ? "Stopping after the current step."
            : "A merge or export that has started finishes first."}
        </p>
      )}

      {running && state.stage !== "merging" && state.stage !== "exporting" && (
        <>
          {position ? (
            <>
              <Bar value={total ? stepsDone(position) / total : undefined} label="Training progress" />
              <p className="mt-1 text-xs tabular-nums text-fog">
                Epoch {position.epoch} of {position.totalEpochs} · step {position.step} of {position.totalSteps}
                {left !== null && ` · about ${formatDuration(left)} left`}
                {last && ` · loss ${last.loss.toFixed(3)}`}
              </p>
            </>
          ) : (
            <>
              <Bar value={undefined} label="Starting" />
              <p className="mt-1 text-xs text-fog">{state.phase ?? "Starting"}…</p>
            </>
          )}
        </>
      )}
      {running && state.stage === "merging" && (
        <>
          <Bar value={state.merge ? state.merge.layer / state.merge.total : undefined} label="Merging" />
          <p className="mt-1 text-xs text-fog">Merging the adapter into the base model…</p>
        </>
      )}
      {running && state.stage === "exporting" && (
        <>
          <Bar value={state.exportProgress ? state.exportProgress.index / state.exportProgress.total : undefined} label="Exporting" />
          <p className="mt-1 text-xs text-fog">Writing the GGUF…</p>
        </>
      )}

      {leftOut !== null && (
        <p className="mt-2 text-xs text-amber-600">
          {leftOut} of {job.datasetRows} rows {leftOut === 1 ? "was" : "were"} left out. A row longer than the max sequence length loses its reply
          to the cut, so it teaches nothing. Raise the max sequence under LoRA settings to use them.
        </p>
      )}
      {skipped > 0 && (
        <p className="mt-2 text-xs text-amber-600">
          {skipped} {skipped === 1 ? "step was" : "steps were"} skipped.
        </p>
      )}
      {state.epochLoss.length > 0 && (
        <p className="mt-2 text-xs tabular-nums text-fog">
          Epoch loss: {state.epochLoss.map((l) => l.toFixed(3)).join(" → ")}
        </p>
      )}
      {steps.length > 1 && <LossChart steps={steps} />}

      {verdict && (
        <div
          role="status"
          className={`mt-3 rounded-md border px-3 py-2 text-xs ${
            verdict.level === "ok"
              ? "border-fog-line"
              : verdict.level === "warn"
                ? "border-amber-600/60"
                : "border-red-600/60"
          }`}
        >
          <p className={`font-medium ${verdict.level === "broken" ? "text-red-600" : verdict.level === "warn" ? "text-amber-600" : ""}`}>
            {verdict.headline}
          </p>
          <p className="mt-0.5 text-fog">{verdict.detail}</p>
        </div>
      )}

      {outcome?.kind === "done" && (
        <p role="status" className="mt-3 text-xs text-fog">
          {census
            ? "Census finished. It trained nothing and made no model."
            : `Finished${outcome.record.gguf_size ? `: ${outcome.record.gguf_path?.split("/").pop()} (${formatBytes(outcome.record.gguf_size)})` : ""}. Find it under Projects.`}
        </p>
      )}
      {outcome?.kind === "failed" && (
        <p role="alert" className="mt-3 text-xs text-red-600">
          {outcome.message}
        </p>
      )}
      {outcome?.kind === "cancelled" && <p role="status" className="mt-3 text-xs text-fog">Stopped. No adapter was written.</p>}
      {sweep && sweep.results.length > 0 && (
        <div className="mt-4">
          <h3 className="text-xs font-medium">{isFinished(sweep) ? "Sweep finished" : "Sweep so far"}</h3>
          <table className="mt-1 w-full text-left text-xs">
            <thead className="text-fog">
              <tr>
                <th className="py-1 pr-3 font-normal">Run</th>
                <th className="py-1 pr-3 font-normal">Result</th>
                <th className="py-1 text-right font-normal">Final loss</th>
              </tr>
            </thead>
            <tbody>
              {sweep.results.map((r) => (
                <tr key={r.name} className="border-t border-fog-line">
                  <td className="py-1 pr-3 font-medium">{r.name}</td>
                  <td className={`py-1 pr-3 ${r.status === "failed" || r.status === "refused" ? "text-red-600" : "text-fog"}`}>
                    {r.status === "done" ? "Done" : r.status === "cancelled" ? r.note : `${r.status === "refused" ? "Not started" : "Failed"}: ${r.note}`}
                  </td>
                  <td className="py-1 text-right tabular-nums">{r.finalLoss === null ? "" : r.finalLoss.toFixed(3)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {isFinished(sweep) && lowestLoss(sweep.results) && (
            <p className="mt-2 text-xs text-fog">
              Lowest training loss: {lowestLoss(sweep.results)}. A low loss on the training set does not mean a better model, so check the replies before you pick one.
            </p>
          )}
        </div>
      )}

      {outcome && !sweepActive && (
        <button
          type="button"
          onClick={job.dismiss}
          className="mt-3 cursor-pointer rounded-md border border-fog-line px-2.5 py-1 text-xs font-medium hover:bg-fog-line"
        >
          Done
        </button>
      )}
    </section>
  );
}
