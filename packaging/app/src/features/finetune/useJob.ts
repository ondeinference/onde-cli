import { useCallback, useEffect, useRef, useState } from "react";
import { ipc } from "@/lib/ipc";
import type { JobRequest, RunEvent, StepRecord } from "@/lib/types";
import { apply, defaultRunName, elapsedFromSteps, idle, positionFromSteps, type JobState } from "./job";

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
        steps.current = recorded;
        // The time estimate and the rows-left-out warning need what the first
        // screen knew: when training began, and how many rows the dataset has.
        const elapsed = elapsedFromSteps(recorded);
        trainingStartedAt.current = elapsed === null ? null : Date.now() - elapsed;
        datasetRows.current = detail?.dataset.state === "ready" ? detail.dataset.rows : null;
        const epochs = detail?.runs.find((r) => r.name === info.run)?.spec?.epochs;
        const position = epochs ? positionFromSteps(recorded, epochs) : null;
        if (position) setState((s) => ({ ...s, phase: null, position }));
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

  const dismiss = useCallback(() => {
    steps.current = [];
    trainingStartedAt.current = null;
    datasetRows.current = null;
    setState(idle);
  }, []);

  return {
    state,
    steps: steps.current,
    trainingStartedAt: trainingStartedAt.current,
    datasetRows: datasetRows.current,
    start,
    cancel,
    dismiss,
  };
}

export type Job = ReturnType<typeof useJob>;
