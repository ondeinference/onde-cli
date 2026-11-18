import { useEffect, useRef, useState } from "react";
import { ipc } from "@/lib/ipc";
import {
  errorMessage,
  isQuietError,
  type DatasetState,
  type ProjectDetail as Detail,
  type RunRecord,
  type StepRecord,
} from "@/lib/types";
import { LossChart } from "./LossChart";
import { formatBytes, formatDate, formatLoss, formatLr } from "./format";

type Message = { role: string; content: string };

/** The first row of a dataset as chat turns. A `{"text"}` row is shown raw. */
function SampleRow({ row }: { row: unknown }) {
  const messages = (row as { messages?: Message[] })?.messages;
  if (!Array.isArray(messages)) {
    const text = String((row as { text?: string })?.text ?? "");
    return <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-fog-line/40 p-3 text-xs">{text}</pre>;
  }
  return (
    <div className="flex flex-col gap-2">
      {messages.map((m, i) => (
        <div
          key={i}
          className={`max-w-[85%] rounded-lg px-3 py-2 text-xs ${
            m.role === "assistant" ? "self-start bg-fog-line/60" : m.role === "user" ? "self-end bg-ink text-paper" : "self-stretch border border-dashed border-fog-line text-fog"
          }`}
        >
          <p className="mb-0.5 text-[10px] uppercase tracking-wider opacity-60">{m.role}</p>
          <p className="line-clamp-4 whitespace-pre-wrap">{m.content}</p>
        </div>
      ))}
    </div>
  );
}

function DatasetCard({
  repoId,
  dataset,
  onReplaced,
}: {
  repoId: string;
  dataset: DatasetState;
  onReplaced: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function replace() {
    setBusy(true);
    setError(null);
    try {
      await ipc.importDataset(repoId);
      onReplaced();
    } catch (e) {
      if (!isQuietError(e)) setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="rounded-lg border border-fog-line px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-medium">Dataset</h2>
        <button
          type="button"
          disabled={busy}
          onClick={() => void replace()}
          className="cursor-pointer rounded-md border border-fog-line px-2.5 py-1 text-xs font-medium hover:bg-fog-line disabled:opacity-50"
        >
          {dataset.state === "missing" ? "Choose dataset…" : "Replace dataset…"}
        </button>
      </div>
      {dataset.state === "missing" && <p className="mt-2 text-xs text-fog">No dataset/train.jsonl yet.</p>}
      {dataset.state === "invalid" && <p className="mt-2 text-xs text-red-600">{dataset.error}</p>}
      {dataset.state === "ready" && (
        <>
          <p className="mt-1 text-xs tabular-nums text-fog">
            {dataset.rows} rows
            {dataset.message_rows > 0 && ` · ${dataset.message_rows} as messages (loss on replies only)`}
            {dataset.text_rows > 0 && ` · ${dataset.text_rows} as raw text`}
            {dataset.message_rows > 0 && ` · ${dataset.with_system} with a system prompt`}
          </p>
          {dataset.sample[0] !== undefined && (
            <div className="mt-3">
              <SampleRow row={dataset.sample[0]} />
            </div>
          )}
        </>
      )}
      {error && (
        <p role="alert" className="mt-2 text-xs text-red-600">
          {error}
        </p>
      )}
    </section>
  );
}

const STATUS: Record<RunRecord["status"], string> = {
  running: "Running",
  done: "Done",
  failed: "Failed",
  cancelled: "Cancelled",
  imported: "Made outside the app",
};

function RunRow({ repoId, run }: { repoId: string; run: RunRecord }) {
  const [open, setOpen] = useState(false);
  const [steps, setSteps] = useState<StepRecord[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const charted = run.status !== "imported";

  useEffect(() => {
    if (!open || steps || !charted) return;
    ipc.runSteps(repoId, run.name).then(setSteps).catch((e) => setError(errorMessage(e)));
  }, [open, steps, charted, repoId, run.name]);

  const cell = "px-3 py-2 tabular-nums";
  return (
    <>
      <tr
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="cursor-pointer border-t border-fog-line hover:bg-fog-line/40"
      >
        <td className={`${cell} font-medium`}>{run.name}</td>
        <td className={`${cell} ${run.status === "failed" ? "text-red-600" : "text-fog"}`}>{STATUS[run.status]}</td>
        <td className={cell}>{run.spec ? run.spec.epochs : ""}</td>
        <td className={cell}>{run.spec ? formatLr(run.spec.learning_rate) : ""}</td>
        <td className={cell}>{formatLoss(run.epoch_loss[run.epoch_loss.length - 1])}</td>
        <td className={`${cell} text-right`}>{formatBytes(run.gguf_size)}</td>
      </tr>
      {open && (
        <tr>
          <td colSpan={6} className="px-3 pb-3 text-xs">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 text-fog">
              {run.backend && (
                <>
                  <dt>Backend</dt>
                  <dd className="text-ink">
                    {run.backend.toUpperCase()}
                    {run.backend_note && <span className="text-fog"> · {run.backend_note}</span>}
                  </dd>
                </>
              )}
              {run.status !== "imported" && (
                <>
                  <dt>Steps</dt>
                  <dd className="text-ink tabular-nums">
                    {run.steps} trained, {run.skipped} skipped
                  </dd>
                </>
              )}
              {run.epoch_loss.length > 0 && (
                <>
                  <dt>Epoch loss</dt>
                  <dd className="text-ink tabular-nums">{run.epoch_loss.map((l) => l.toFixed(3)).join(" → ")}</dd>
                </>
              )}
              {run.gguf_path && (
                <>
                  <dt>GGUF</dt>
                  <dd className="truncate font-mono text-ink" title={run.gguf_path}>
                    {run.gguf_path.split("/").pop()}
                  </dd>
                </>
              )}
              {run.finished_at != null && (
                <>
                  <dt>Finished</dt>
                  <dd className="text-ink">{formatDate(run.finished_at)}</dd>
                </>
              )}
              {run.error && (
                <>
                  <dt>Error</dt>
                  <dd className="text-red-600">{run.error}</dd>
                </>
              )}
            </dl>
            {!charted && <p className="mt-2 text-fog">This run was made by hand, so it has no step history to chart.</p>}
            {error && <p className="mt-2 text-red-600">{error}</p>}
            {steps && <LossChart steps={steps} />}
          </td>
        </tr>
      )}
    </>
  );
}

export function ProjectDetail({ repoId, onBack }: { repoId: string; onBack: () => void }) {
  const [detail, setDetail] = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Only the newest request may fill the page. Without this, a slow answer for
  // project A that lands after B was opened would show A's data under B's name.
  const latest = useRef(0);

  function load() {
    const mine = ++latest.current;
    ipc
      .projectDetail(repoId)
      .then((d) => mine === latest.current && setDetail(d))
      .catch((e) => mine === latest.current && setError(errorMessage(e)));
  }
  useEffect(() => {
    setDetail(null);
    setError(null);
    load();
    return () => {
      latest.current++;
    };
  }, [repoId]);

  return (
    <div className="flex flex-col gap-4">
      <div>
        <button type="button" onClick={onBack} className="cursor-pointer text-xs text-fog hover:text-ink">
          ← Projects
        </button>
        <h1 className="mt-1 text-lg font-semibold">{repoId}</h1>
        {detail && (
          <p className="text-xs text-fog">
            Base model {detail.base_model_id}
            {detail.created_at ? ` · created ${formatDate(detail.created_at)}` : ""}
          </p>
        )}
      </div>
      {error && <p className="text-sm text-red-600">{error}</p>}
      {detail && (
        <>
          <DatasetCard repoId={repoId} dataset={detail.dataset} onReplaced={load} />
          <section>
            <h2 className="mb-2 text-sm font-medium">Runs</h2>
            {detail.runs.length === 0 ? (
              <p className="text-xs text-fog">No runs yet.</p>
            ) : (
              <div className="overflow-x-auto rounded-lg border border-fog-line">
                <table className="w-full text-left text-xs">
                  <thead className="text-fog">
                    <tr>
                      <th className="px-3 py-2 font-normal">Run</th>
                      <th className="px-3 py-2 font-normal">Status</th>
                      <th className="px-3 py-2 font-normal">Epochs</th>
                      <th className="px-3 py-2 font-normal">LR</th>
                      <th className="px-3 py-2 font-normal">Final loss</th>
                      <th className="px-3 py-2 text-right font-normal">GGUF</th>
                    </tr>
                  </thead>
                  <tbody>
                    {detail.runs.map((r) => (
                      <RunRow key={r.name} repoId={repoId} run={r} />
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
