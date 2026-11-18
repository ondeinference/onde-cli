// Planning and running a sweep: the same training run over several epoch counts
// and learning rates. Pure functions, so they can be tested without a window.
import type { JobRequest, RunStatus } from "../../lib/types.ts";
import { defaultRunName } from "./job.ts";

/** More than this is almost certainly a typo, and each run can take an hour. */
export const MAX_SWEEP_RUNS = 24;

export type ListResult = { ok: true; values: number[] } | { ok: false; error: string };

/** "2, 3" or "5e-5 1e-4" into numbers. Duplicates are dropped, order is kept. */
export function parseList(text: string, label: string, valid: (n: number) => boolean, rule: string): ListResult {
  const parts = text
    .split(/[\s,;]+/)
    .map((p) => p.trim())
    .filter((p) => p !== "");
  if (parts.length === 0) return { ok: false, error: `${label}: enter at least one value.` };
  const values: number[] = [];
  for (const part of parts) {
    const n = Number(part);
    if (!Number.isFinite(n) || !valid(n)) return { ok: false, error: `${label}: "${part}" is not valid. ${rule}` };
    if (!values.includes(n)) values.push(n);
  }
  return { ok: true, values };
}

export const parseEpochs = (text: string) =>
  parseList(text, "Epochs", (n) => Number.isInteger(n) && n >= 1 && n <= 100, "Use whole numbers from 1 to 100.");
export const parseRates = (text: string) =>
  parseList(text, "Learning rates", (n) => n > 0 && n <= 1, "Use numbers above 0 and at most 1, like 1e-4.");

export interface Planned {
  request: JobRequest;
  name: string;
}

export interface SweepPlan {
  /** Runs to do, in order: every epoch count at each learning rate in turn. */
  runs: Planned[];
  /** Names that already exist in the project. They are left alone. */
  skipped: string[];
}

/** The grid of runs for a base request, minus the ones that already exist. */
export function planSweep(
  base: JobRequest,
  epochs: number[],
  rates: number[],
  existing: string[],
): SweepPlan {
  const taken = new Set(existing);
  const runs: Planned[] = [];
  const skipped: string[] = [];
  for (const learning_rate of rates) {
    for (const ep of epochs) {
      const name = defaultRunName(ep, learning_rate);
      if (taken.has(name)) {
        skipped.push(name);
        continue;
      }
      taken.add(name);
      runs.push({ name, request: { ...base, kind: "run", name, epochs: ep, learning_rate } });
    }
  }
  return { runs, skipped };
}

export interface SweepResult {
  name: string;
  status: RunStatus | "refused" | "skipped";
  /** Mean loss of the last epoch, when the run got that far. */
  finalLoss: number | null;
  /** Why it did not finish, when it did not. */
  note: string | null;
}

export interface SweepState {
  total: number;
  /** Requests still to start, in order. */
  queue: JobRequest[];
  /** What each finished or refused run came to. */
  results: SweepResult[];
  /** The user asked to stop. The queue is emptied as soon as the run in flight
   *  has ended, so nothing more starts after it. */
  stopped: boolean;
}

export function begin(plan: SweepPlan): SweepState | null {
  if (plan.runs.length === 0) return null;
  return {
    total: plan.runs.length,
    queue: plan.runs.map((r) => r.request),
    results: [],
    stopped: false,
  };
}

/** What the header says: where a sweep in progress is, or how far a finished one
 *  got. A stopped sweep did not reach its last runs, so it must not claim to. */
export function headline(s: SweepState): string {
  if (isStopping(s)) return `Stopping after run ${position(s)} of ${s.total}`;
  if (!isFinished(s)) return `Sweep, run ${position(s)} of ${s.total}`;
  return s.results.length < s.total
    ? `Sweep stopped after ${s.results.length} of ${s.total} runs`
    : `Sweep of ${s.total} ${s.total === 1 ? "run" : "runs"}`;
}

/** 1-based number of the run in progress. */
export const position = (s: SweepState): number => s.results.length + 1;
/** Nothing is left to start. A stopped sweep is not finished until the run that
 *  was in flight has ended, so the screen does not claim it is over (or swap the
 *  Stop button for Cancel) while that run is still winding down. */
export const isFinished = (s: SweepState): boolean => s.queue.length === 0;

/** Stop was asked for and the run in flight has not ended yet. */
export const isStopping = (s: SweepState): boolean => s.stopped && !isFinished(s);

/** The user pressed Stop. With a run in flight, it is allowed to end first
 *  (training stops after its step, a merge or export finishes). With none, which
 *  is the instant between two runs, there is nothing to wait for, so the sweep
 *  ends now. */
export function requestStop(s: SweepState, runInFlight: boolean): SweepState {
  return { ...s, stopped: true, queue: runInFlight ? s.queue : [] };
}

export type Outcome =
  | { kind: "done"; finalLoss: number | null }
  | { kind: "failed"; message: string }
  | { kind: "cancelled" };

/** Record how the current run ended and move to the next one. Returns the new
 *  state; the caller starts `state.queue[0]` unless the sweep is finished. */
export function advance(s: SweepState, name: string, outcome: Outcome): SweepState {
  const result: SweepResult =
    outcome.kind === "done"
      ? { name, status: "done", finalLoss: outcome.finalLoss, note: null }
      : outcome.kind === "failed"
        ? { name, status: "failed", finalLoss: null, note: outcome.message }
        : { name, status: "cancelled", finalLoss: null, note: s.stopped ? "Sweep stopped" : "Skipped" };
  // Cancelling a run is either "skip this one" or "stop everything", and the
  // note above follows `stopped`. Anything else that ends a run, including a
  // failure, carries on with the rest. A stop empties the queue, whatever the
  // run ended with: it may have finished before the cancel landed.
  return {
    ...s,
    queue: s.stopped ? [] : s.queue.slice(1),
    results: [...s.results, result],
  };
}

/** A request the backend refused to start: record it and move on. */
export function refuse(s: SweepState, name: string, message: string): SweepState {
  return {
    ...s,
    queue: s.stopped ? [] : s.queue.slice(1),
    results: [...s.results, { name, status: "refused", finalLoss: null, note: message }],
  };
}

/** The name of the run with the lowest final loss. A lower loss on the training
 *  set is not a better model, so this is only a pointer, never a verdict. */
export function lowestLoss(results: SweepResult[]): string | null {
  let best: SweepResult | null = null;
  for (const r of results) {
    if (r.finalLoss !== null && (best === null || r.finalLoss < (best.finalLoss as number))) best = r;
  }
  return best?.name ?? null;
}
