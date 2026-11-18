import { test } from "node:test";
import assert from "node:assert/strict";
import { apply, begin, defaultRunName, elapsedFromSteps, positionFromSteps, formatDuration, idle, isRunning, rowsLeftOut, secondsLeft, stepsDone } from "./job.ts";
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
  const done = apply(stopping, { event: "done", name: "r", status: "done", epoch_loss: [] } as unknown as RunEvent & RunRecord);
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

test("the default run name matches the backend's, including exponents that Rust writes without a sign", () => {
  assert.equal(defaultRunName(2, 1e-4), "e2-lr1e-4");
  assert.equal(defaultRunName(3, 5e-5), "e3-lr5e-5");
  assert.equal(defaultRunName(2, 1.5e-4), "e2-lr1.5e-4");
  assert.equal(defaultRunName(1, 0.05), "e1-lr5e-2");
  assert.equal(defaultRunName(1, 1), "e1-lr1e0");
});

test("time spent training is read off the recorded steps when a run is picked back up", () => {
  assert.equal(elapsedFromSteps([]), null);
  assert.equal(elapsedFromSteps([{ t_ms: 2000 }, { t_ms: 2500 }, { t_ms: 9000 }]), 7000);
});

test("training position is rebuilt from recorded steps once the first epoch is over", () => {
  const rec = (epoch: number, step: number) => ({ epoch, step });
  const firstEpoch = [rec(1, 1), rec(1, 2), rec(1, 3), rec(1, 4)];
  assert.equal(positionFromSteps([], 2), null);
  assert.equal(positionFromSteps(firstEpoch.slice(0, 2), 2), null, "steps per epoch are not known mid first epoch");
  assert.equal(positionFromSteps(firstEpoch, 2), null, "an epoch that has just ended is not yet proof of its length");
  assert.deepEqual(positionFromSteps([...firstEpoch, rec(2, 1), rec(2, 2)], 2), {
    epoch: 2,
    totalEpochs: 2,
    step: 2,
    totalSteps: 4,
  });
  assert.equal(positionFromSteps(firstEpoch, 0), null);
});

test("a finished run fills in the epoch losses a late-attached screen missed, and never shortens them", () => {
  const record = { event: "done", name: "r", status: "done", epoch_loss: [0.9, 0.4] } as unknown as RunEvent;
  const late = apply(begin(info), record);
  assert.deepEqual(late.epochLoss, [0.9, 0.4]);
  const watched = apply({ ...begin(info), epochLoss: [0.9, 0.4, 0.2] }, record);
  assert.deepEqual(watched.epochLoss, [0.9, 0.4, 0.2]);
});
