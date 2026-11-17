import type { StepRecord } from "@/lib/types";

const W = 560;
const H = 96;
const PAD = 4;

function path(values: (number | null)[], log: boolean): string {
  const points = values
    .map((v, i) => [i, v] as const)
    .filter((p): p is readonly [number, number] => p[1] != null && Number.isFinite(p[1]) && (!log || p[1] > 0));
  if (points.length < 2) return "";
  const ys = points.map(([, v]) => (log ? Math.log10(v) : v));
  const lo = Math.min(...ys);
  const hi = Math.max(...ys);
  const span = hi - lo || 1;
  const last = values.length - 1 || 1;
  return points
    .map(([i], k) => {
      const x = PAD + (i / last) * (W - 2 * PAD);
      const y = PAD + (1 - (ys[k] - lo) / span) * (H - 2 * PAD);
      return `${k === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

function Line({ label, d, extra }: { label: string; d: string; extra: string }) {
  return (
    <figure className="mt-3">
      <figcaption className="flex justify-between text-xs text-fog">
        <span>{label}</span>
        <span className="tabular-nums">{extra}</span>
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} className="mt-1 h-24 w-full" role="img" aria-label={label} preserveAspectRatio="none">
        <path d={d} fill="none" stroke="currentColor" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
      </svg>
    </figure>
  );
}

/** Per-step loss, and grad norm on a log scale when the run recorded it.
 *  Skipped steps are counted, not drawn. */
export function LossChart({ steps }: { steps: StepRecord[] }) {
  const kept = steps.filter((s) => s.skipped == null);
  const loss = kept.map((s) => s.loss);
  const norms = kept.map((s) => s.grad_norm);
  const lossPath = path(loss, false);
  const normPath = path(norms, true);
  const finiteNorms = norms.filter((n): n is number => n != null && Number.isFinite(n));
  const skipped = steps.length - kept.length;

  if (!lossPath) return <p className="mt-3 text-xs text-fog">Not enough steps to chart.</p>;
  return (
    <div>
      <Line
        label={`Loss per step${skipped ? ` (${skipped} skipped)` : ""}`}
        d={lossPath}
        extra={`${loss[0].toFixed(2)} → ${loss[loss.length - 1].toFixed(2)}`}
      />
      {normPath && (
        <Line
          label="Gradient norm (log scale)"
          d={normPath}
          extra={`${Math.min(...finiteNorms).toFixed(1)} – ${Math.max(...finiteNorms).toFixed(1)}`}
        />
      )}
    </div>
  );
}
