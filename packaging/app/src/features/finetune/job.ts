// What the Fine-tune screen knows about the running job, as pure functions so
// they can be tested without a window. Only `import type` here: the node test
// runner strips types and does not resolve the "@/" alias.
import type { Backend, JobInfo, RunEvent, RunRecord, Stage } from "../../lib/types.ts";

export interface Position {
  epoch: number;
  totalEpochs: number;
  step: number;
  totalSteps: number;
}

export type Outcome =
  | { kind: "done"; record: RunRecord }
  | { kind: "failed"; message: string }
  | { kind: "cancelled" };

export interface JobState {
  info: JobInfo | null;
  stage: Stage | null;
  backend: { name: Backend; fallback: string | null } | null;
  /** Loading, tokenising and saving have no numbers worth charting. */
  phase: string | null;
  position: Position | null;
  epochLoss: number[];
  merge: { layer: number; total: number } | null;
  exportProgress: { index: number; total: number } | null;
  /** Set once the run ends, and kept until the next one starts. */
  outcome: Outcome | null;
  /** The user asked to stop, and the run has not yet said it did. */
  stopping: boolean;
}

export const idle: JobState = {
  info: null,
  stage: null,
  backend: null,
  phase: null,
  position: null,
  epochLoss: [],
  merge: null,
  exportProgress: null,
  outcome: null,
  stopping: false,
};

export function begin(info: JobInfo): JobState {
  return { ...idle, info, phase: "Starting" };
}

export const isRunning = (s: JobState): boolean => s.info !== null && s.outcome === null;

/** Fold one event into the state. Steps are not kept here: a long run has far
 *  too many to copy on every event, so the hook collects them separately. */
export function apply(s: JobState, e: RunEvent): JobState {
  switch (e.event) {
    case "stage":
      return { ...s, stage: e.stage, phase: null };
    case "backend":
      return { ...s, backend: { name: e.backend, fallback: e.fallback } };
    case "phase":
      return { ...s, phase: e.label };
    case "training":
      return {
        ...s,
        phase: null,
        position: { epoch: e.epoch, totalEpochs: e.total_epochs, step: e.step, totalSteps: e.total_steps },
      };
    case "epoch_done":
      return { ...s, epochLoss: [...s.epochLoss, e.mean_loss] };
    case "merge":
      return { ...s, merge: { layer: e.layer, total: e.total } };
    case "export":
      return { ...s, exportProgress: { index: e.index, total: e.total } };
    case "done":
      return {
        ...s,
        stopping: false,
        // A screen that attached mid-run missed the epoch events. The finished
        // record has them all.
        epochLoss: e.epoch_loss.length > s.epochLoss.length ? e.epoch_loss : s.epochLoss,
        outcome: { kind: "done", record: e },
      };
    case "failed":
      return { ...s, stopping: false, outcome: { kind: "failed", message: e.message } };
    case "cancelled":
      return { ...s, stopping: false, outcome: { kind: "cancelled" } };
    case "step":
      return s;
  }
}

/** Optimizer steps finished so far, across all epochs. */
export function stepsDone(p: Position): number {
  return (p.epoch - 1) * p.totalSteps + p.step;
}

/** Seconds left in training, from the rate so far. `null` until there is a rate
 *  to go on. `elapsedMs` is the time since the first step. */
export function secondsLeft(p: Position, elapsedMs: number): number | null {
  const done = stepsDone(p);
  if (done < 3 || elapsedMs <= 0) return null;
  const total = p.totalEpochs * p.totalSteps;
  return Math.max(0, Math.round(((total - done) * elapsedMs) / done / 1000));
}

/** Rows the trainer left out: those longer than the max sequence length lose
 *  their reply to the cut, and rows that are too short have nothing to learn
 *  from. The trainer only logs this, so the screen says it. `null` when nothing
 *  was left out or the counts are not known. */
export function rowsLeftOut(datasetRows: number | null, p: Position | null): number | null {
  if (datasetRows === null || p === null) return null;
  const left = datasetRows - p.totalSteps;
  return left > 0 ? left : null;
}

/** The name a run gets when the form leaves it empty. Matches `default_run_name`
 *  in the backend, so the header shows the real name before the backend replies.
 *  Rust's `{:e}` and `toExponential()` agree once the sign of the exponent is
 *  written the same way. */
export function defaultRunName(epochs: number, learningRate: number): string {
  return `e${epochs}-lr${learningRate.toExponential().replace("e+", "e")}`;
}

/** Where a run that is already going stands, from the steps it has recorded.
 *  `elapsedMs` is how long training has been going, from the first step to the
 *  latest. `null` when no step has been recorded yet. */
export function elapsedFromSteps(steps: { t_ms: number }[]): number | null {
  if (steps.length === 0) return null;
  return steps[steps.length - 1].t_ms - steps[0].t_ms;
}

/** Where training stands, rebuilt from the steps a run has recorded. The step
 *  events that carried this are not replayed when a screen attaches late.
 *
 *  A step is recorded for every row the trainer used, so the first epoch's last
 *  step number is the steps per epoch. Until the first epoch is over that is not
 *  known, and this returns `null` rather than guess. */
export function positionFromSteps(
  steps: { epoch: number; step: number }[],
  totalEpochs: number,
): Position | null {
  if (steps.length === 0 || totalEpochs < 1) return null;
  const last = steps[steps.length - 1];
  const firstEpoch = steps.filter((s) => s.epoch === 1);
  const firstEpochDone = last.epoch > 1;
  // The trainer is on its last epoch's last step, or past training, when the
  // run has as many epochs recorded as it was asked for.
  const totalSteps = firstEpochDone
    ? firstEpoch.reduce((m, s) => Math.max(m, s.step), 0)
    : null;
  if (totalSteps === null) return null;
  return { epoch: last.epoch, totalEpochs, step: last.step, totalSteps };
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  if (m < 60) return `${m}m ${seconds % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}
