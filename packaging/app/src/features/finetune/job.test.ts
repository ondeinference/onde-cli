import { test } from "node:test";
import assert from "node:assert/strict";
import { apply, begin, formatDuration, idle, isRunning, rowsLeftOut, secondsLeft, stepsDone } from "./job.ts";
import type { JobInfo, RunEvent, RunRecord } from "../../lib/types.ts";

const info: JobInfo = { repo_id: "acme/tiny", run: "e2-lr1e-4", kind: "run" };
const training = (epoch: number, step: number): RunEvent => ({
  event: "training",
  epoch,
  total_epochs: 2,
  step,
  total_steps: 4,
  loss: 1,
});

test("a started job is running until it has an outcome", () => {
  assert.equal(isRunning(idle), false);
  const s = begin(info);
  assert.equal(isRunning(s), true);
  assert.equal(s.phase, "Starting");
});

test("events fill in the stage, backend, position and epoch losses", () => {
  let s = begin(info);
  s = apply(s, { event: "backend", backend: "cpu", fallback: "MLX is unavailable: no metallib" });
  s = apply(s, { event: "stage", stage: "training" });
  s = apply(s, training(2, 3));
  s = apply(s, { event: "epoch_done", epoch: 1, mean_loss: 0.9 });
  s = apply(s, { event: "epoch_done", epoch: 2, mean_loss: 0.4 });
  assert.deepEqual(s.backend, { name: "cpu", fallback: "MLX is unavailable: no metallib" });
  assert.equal(s.stage, "training");
  assert.equal(s.phase, null);
  assert.deepEqual(s.epochLoss, [0.9, 0.4]);
  assert.equal(stepsDone(s.position!), 7);
});

test("a step event changes nothing: steps are collected outside the state", () => {
  const s = begin(info);
  const e: RunEvent = { event: "step", epoch: 1, step: 1, loss: 1, grad_norm: 2, skipped: null, t_ms: 3 };
  assert.equal(apply(s, e), s);
});

test("each way a run can end is recorded and clears the stopping flag", () => {
  const stopping = { ...begin(info), stopping: true };
  const done = apply(stopping, { event: "done", name: "r", status: "done" } as RunEvent & RunRecord);
  assert.equal(done.outcome?.kind, "done");
  assert.equal(done.stopping, false);
  assert.equal(isRunning(done), false);
  assert.deepEqual(apply(stopping, { event: "failed", message: "boom" }).outcome, { kind: "failed", message: "boom" });
  assert.deepEqual(apply(stopping, { event: "cancelled" }).outcome, { kind: "cancelled" });
});

test("time left needs a few steps, then follows the rate", () => {
  const at = (epoch: number, step: number) => ({ epoch, totalEpochs: 2, step, totalSteps: 4 });
  assert.equal(secondsLeft(at(1, 2), 2000), null, "too early to guess");
  // 4 of 8 steps in 40s: 40 more seconds.
  assert.equal(secondsLeft(at(1, 4), 40_000), 40);
  assert.equal(secondsLeft(at(2, 4), 80_000), 0);
  assert.equal(secondsLeft(at(1, 4), 0), null);
});

test("durations read as seconds, minutes or hours", () => {
  assert.equal(formatDuration(45), "45s");
  assert.equal(formatDuration(125), "2m 5s");
  assert.equal(formatDuration(3725), "1h 2m");
});

test("rows the trainer left out are counted, and only when it used fewer than the dataset has", () => {
  const p = { epoch: 1, totalEpochs: 2, step: 1, totalSteps: 4 };
  assert.equal(rowsLeftOut(12, p), 8);
  assert.equal(rowsLeftOut(4, p), null, "every row was used");
  assert.equal(rowsLeftOut(null, p), null, "dataset size unknown");
  assert.equal(rowsLeftOut(12, null), null, "no step yet");
});
