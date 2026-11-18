import { useCallback, useEffect, useRef, useState } from "react";
import { ipc } from "@/lib/ipc";
import type { JobRequest, RunEvent, StepRecord } from "@/lib/types";
import { apply, defaultRunName, elapsedFromSteps, idle, positionFromSteps, type JobState } from "./job";
import * as sweepLogic from "./sweep";
import type { SweepPlan, SweepState } from "./sweep";

/** The running job as the screen sees it. It lives in `App`, so it survives
 *  switching tabs, and it reattaches to a job that kept running while the
 *  webview reloaded. */
export function useJob() {
  const [state, setState] = useState<JobState>(idle);
  // A long run has too many steps to copy into state on every event, so they
  // are collected here and the screen is told to redraw at most a few times a second.
  const steps = useRef<StepRecord[]>([]);
  const trainingStartedAt = useRef<number | null>(null);
  // How many rows the dataset has, to notice when the trainer used fewer.
  const datasetRows = useRef<number | null>(null);
  const [, redraw] = useState(0);
  const timer = useRef<number | undefined>(undefined);
  // The queue of a sweep. It lives in a ref so the event handlers always see the
  // current one, and is copied to state for the screen to draw. It is not kept
  // across a reload: the run in progress reattaches, but the ones after it do not.
  const sweepRef = useRef<SweepState | null>(null);
  const [sweep, setSweepState] = useState<SweepState | null>(null);
  const handledOutcome = useRef<unknown>(null);
  const setSweep = useCallback((next: SweepState | null) => {
    sweepRef.current = next;
    setSweepState(next);
  }, []);

  const scheduleRedraw = useCallback(() => {
    if (timer.current !== undefined) return;
    timer.current = window.setTimeout(() => {
      timer.current = undefined;
      redraw((n) => n + 1);
    }, 200);
  }, []);

  const onEvent = useCallback(
    (e: RunEvent) => {
      if (e.event === "step") {
        steps.current.push(e);
        scheduleRedraw();
        return;
      }
      if (e.event === "training" && trainingStartedAt.current === null) {
        trainingStartedAt.current = Date.now();
      }
      setState((s) => apply(s, e));
    },
    [scheduleRedraw],
  );

  useEffect(() => {
    ipc
      .attachJob(onEvent)
      .then(async (info) => {
        if (!info) return;
        setState({
          ...idle,
          info,
          stage: info.stage,
          backend: info.backend ? { name: info.backend.backend, fallback: info.backend.fallback } : null,
          phase: info.stage ? null : "Reconnected to the running job",
        });
        const [recorded, detail] = await Promise.all([
          ipc.runSteps(info.repo_id, info.run).catch(() => []),
          ipc.projectDetail(info.repo_id).catch(() => null),
        ]);
        // Steps that arrived on the live channel while those fetches ran are in
        // steps.current already. Keep them, and drop any the file also has.
        const seen = new Set(recorded.map((r) => `${r.epoch}:${r.step}`));
        const merged = [...recorded, ...steps.current.filter((r) => !seen.has(`${r.epoch}:${r.step}`))];
        steps.current = merged;
        // The time estimate and the rows-left-out warning need what the first
        // screen knew: when training began, and how many rows the dataset has.
        const elapsed = elapsedFromSteps(merged);
        trainingStartedAt.current = elapsed === null ? null : Date.now() - elapsed;
        datasetRows.current = detail?.dataset.state === "ready" ? detail.dataset.rows : null;
        const epochs = detail?.runs.find((r) => r.name === info.run)?.spec?.epochs;
        const position = epochs ? positionFromSteps(merged, epochs) : null;
        if (position) setState((s) => (s.position ? { ...s, phase: null } : { ...s, phase: null, position }));
        redraw((n) => n + 1);
      })
      .catch(() => {});
    return () => {
      if (timer.current !== undefined) window.clearTimeout(timer.current);
    };
  }, [onEvent]);

  /** Start a job. Rejects, leaving the state idle, when the backend refuses. */
  const start = useCallback(
    async (request: JobRequest, rows: number | null) => {
      steps.current = [];
      trainingStartedAt.current = null;
      datasetRows.current = rows;
      setState({
        ...idle,
        info: {
          repo_id: request.repo_id,
          run:
            request.kind === "census"
              ? "census"
              : request.name || defaultRunName(request.epochs, request.learning_rate),
          kind: request.kind,
          stage: null,
          backend: null,
        },
        phase: "Starting",
      });
      try {
        const info = await ipc.startJob(request, onEvent);
        setState((s) => ({ ...s, info }));
      } catch (e) {
        setState(idle);
        throw e;
      }
    },
    [onEvent],
  );

  const cancel = useCallback(() => {
    setState((s) => ({ ...s, stopping: true }));
    void ipc.cancelJob();
  }, []);

  /** Start the next queued run of the sweep, recording any the backend refuses. */
  const launchNext = useCallback(async () => {
    for (;;) {
      const current = sweepRef.current;
      if (!current || sweepLogic.isFinished(current)) return;
      const request = current.queue[0];
      try {
        await start(request, datasetRows.current);
        return;
      } catch (e) {
        const message = e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e);
        setSweep(sweepLogic.refuse(current, request.name, message));
      }
    }
  }, [start, setSweep]);

  /** Run every planned run in turn. `rows` is the dataset size, for the warning
   *  about rows the trainer leaves out. */
  const startSweep = useCallback(
    async (plan: SweepPlan, rows: number | null) => {
      const first = sweepLogic.begin(plan);
      if (!first) return;
      datasetRows.current = rows;
      handledOutcome.current = null;
      setSweep(first);
      await launchNext();
    },
    [launchNext, setSweep],
  );

  // When a run of a sweep ends, record how it went and start the next.
  useEffect(() => {
    const outcome = state.outcome;
    const current = sweepRef.current;
    if (!outcome || !current || handledOutcome.current === outcome) return;
    handledOutcome.current = outcome;
    const name = state.info?.run ?? "";
    const next = sweepLogic.advance(
      current,
      name,
      outcome.kind === "done"
        ? { kind: "done", finalLoss: outcome.record.epoch_loss.at(-1) ?? null }
        : outcome.kind === "failed"
          ? { kind: "failed", message: outcome.message }
          : { kind: "cancelled" },
    );
    setSweep(next);
    if (!sweepLogic.isFinished(next)) void launchNext();
  }, [state.outcome, state.info, launchNext, setSweep]);

  /** Cancel the current run and carry on with the rest of the sweep. */
  const skipRun = useCallback(() => {
    const current = sweepRef.current;
    if (current) setSweep({ ...current, skipCurrent: true });
    cancel();
  }, [cancel, setSweep]);

  /** Cancel the current run and start no more. */
  const stopSweep = useCallback(() => {
    const current = sweepRef.current;
    if (current) setSweep({ ...current, stopped: true });
    cancel();
  }, [cancel, setSweep]);

  const dismiss = useCallback(() => {
    steps.current = [];
    trainingStartedAt.current = null;
    datasetRows.current = null;
    setSweep(null);
    setState(idle);
  }, [setSweep]);

  return {
    state,
    steps: steps.current,
    trainingStartedAt: trainingStartedAt.current,
    datasetRows: datasetRows.current,
    sweep,
    start,
    startSweep,
    skipRun,
    stopSweep,
    cancel,
    dismiss,
  };
}

export type Job = ReturnType<typeof useJob>;
