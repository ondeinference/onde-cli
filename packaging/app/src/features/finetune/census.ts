import type { StepRecord } from "../../lib/types.ts";

export interface Verdict {
  level: "ok" | "warn" | "broken";
  headline: string;
  detail: string;
}

/** Gradient norms at or above this mean the backward pass is producing garbage.
 *  Candle's Metal backward gave 1e6 to 1e29 where the CPU gave 40 to 65. */
export const BROKEN_NORM = 1e6;
/** Healthy norms are around 10 to 100. Well past that is worth a look. */
export const HIGH_NORM = 500;

/** Judge a gradient census: one epoch at a learning rate of 0. `null` until
 *  there is a step to judge. */
export function censusVerdict(steps: StepRecord[]): Verdict | null {
  if (steps.length === 0) return null;
  const norms = steps.map((s) => s.grad_norm).filter((n): n is number => n != null && Number.isFinite(n));
  const skipped = steps.filter((s) => s.skipped != null).length;
  const max = norms.length ? Math.max(...norms) : 0;
  const min = norms.length ? Math.min(...norms) : 0;
  const range = `${min.toFixed(1)} to ${max.toFixed(1)}`;

  if (norms.length === 0) {
    return {
      level: "broken",
      headline: "No usable gradients",
      detail:
        skipped === steps.length
          ? `All ${steps.length} steps were skipped. The backward pass is not producing numbers the optimizer can use.`
          : `The ${steps.length} steps ran, but none reported a finite gradient norm, so there is nothing to judge. Check the backend before training.`,
    };
  }
  if (max >= BROKEN_NORM) {
    return {
      level: "broken",
      headline: "Broken backward pass",
      detail: `Gradient norms reached ${max.toExponential(1)}, where a healthy run is around 10 to 100. Do not train on this setup.`,
    };
  }
  if (skipped > 0) {
    return {
      level: "warn",
      headline: `${skipped} of ${steps.length} steps were skipped`,
      detail: `Norms on the rest were ${range}. Skipped steps are rows with no loss to learn from, or non-finite values. Check the dataset.`,
    };
  }
  if (max > HIGH_NORM) {
    return {
      level: "warn",
      headline: "Gradient norms are high",
      detail: `They ran ${range}, above the usual 10 to 100. Training clips them, but watch the first epoch's loss.`,
    };
  }
  return {
    level: "ok",
    headline: "Gradients look healthy",
    detail: `Norms ran ${range} over ${steps.length} steps, and none were skipped.`,
  };
}
