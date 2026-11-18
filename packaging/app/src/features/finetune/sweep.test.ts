import { test } from "node:test";
import assert from "node:assert/strict";
import { advance, begin, headline, isFinished, isStopping, lowestLoss, parseEpochs, parseRates, planSweep, position, refuse, requestStop } from "./sweep.ts";
import type { JobRequest } from "../../lib/types.ts";

const base: JobRequest = {
  repo_id: "acme/tiny",
  kind: "run",
  name: "",
  learning_rate: 1e-4,
  epochs: 2,
  lora_rank: 8,
  lora_alpha: 16,
  max_seq_len: 512,
  dtype: "q4_k_m",
};

test("lists accept commas, spaces and semicolons, drop duplicates and keep order", () => {
  assert.deepEqual(parseEpochs("2, 3  3;4"), { ok: true, values: [2, 3, 4] });
  assert.deepEqual(parseRates("5e-5 1e-4"), { ok: true, values: [5e-5, 1e-4] });
});

test("bad values are named in the error", () => {
  assert.match((parseEpochs("2, 2.5") as { error: string }).error, /"2\.5" is not valid/);
  assert.match((parseEpochs("0") as { error: string }).error, /"0"/);
  assert.match((parseEpochs("101") as { error: string }).error, /"101"/);
  assert.match((parseRates("1e-4, fast") as { error: string }).error, /"fast"/);
  assert.match((parseRates("0") as { error: string }).error, /"0"/);
  assert.match((parseRates("2") as { error: string }).error, /"2"/);
  assert.equal(parseEpochs("  ").ok, false);
});

test("the grid goes through each learning rate in turn and names runs the usual way", () => {
  const plan = planSweep(base, [2, 3], [5e-5, 1e-4], []);
  assert.deepEqual(plan.runs.map((r) => r.name), ["e2-lr5e-5", "e3-lr5e-5", "e2-lr1e-4", "e3-lr1e-4"]);
  const r = plan.runs[1].request;
  assert.deepEqual([r.kind, r.epochs, r.learning_rate, r.name, r.lora_rank], ["run", 3, 5e-5, "e3-lr5e-5", 8]);
});

test("runs that already exist are skipped and reported, not repeated", () => {
  const plan = planSweep(base, [2, 3], [1e-4], ["e2-lr1e-4", "census-1"]);
  assert.deepEqual(plan.runs.map((r) => r.name), ["e3-lr1e-4"]);
  assert.deepEqual(plan.skipped, ["e2-lr1e-4"]);
});

test("a grid that collapses to a single name does not repeat it", () => {
  assert.equal(planSweep(base, [2, 2], [1e-4], []).runs.length, 1);
});

test("a sweep with nothing to run does not start", () => {
  assert.equal(begin(planSweep(base, [2], [1e-4], ["e2-lr1e-4"])), null);
});

test("each finished run is recorded and the next one comes up", () => {
  let s = begin(planSweep(base, [2, 3], [1e-4], []))!;
  assert.equal(position(s), 1);
  s = advance(s, "e2-lr1e-4", { kind: "done", finalLoss: 0.5 });
  assert.equal(position(s), 2);
  assert.equal(s.queue.length, 1);
  assert.equal(isFinished(s), false);
  s = advance(s, "e3-lr1e-4", { kind: "done", finalLoss: 0.3 });
  assert.equal(isFinished(s), true);
  assert.deepEqual(s.results.map((r) => [r.name, r.status, r.finalLoss]), [
    ["e2-lr1e-4", "done", 0.5],
    ["e3-lr1e-4", "done", 0.3],
  ]);
});

test("a failed run is recorded with its reason and the sweep carries on", () => {
  let s = begin(planSweep(base, [2, 3], [1e-4], []))!;
  s = advance(s, "e2-lr1e-4", { kind: "failed", message: "out of memory" });
  assert.equal(isFinished(s), false);
  assert.deepEqual(s.results[0], { name: "e2-lr1e-4", status: "failed", finalLoss: null, note: "out of memory" });
});

test("skipping a run carries on with the rest", () => {
  let s = begin(planSweep(base, [2, 3, 4], [1e-4], []))!;
  s = advance(s, "e2-lr1e-4", { kind: "cancelled" });
  assert.equal(s.results[0].note, "Skipped");
  assert.equal(isFinished(s), false);
  assert.equal(s.queue.length, 2);
});

test("a stop waits for the run in flight: the sweep is stopping, not finished", () => {
  let s = begin(planSweep(base, [2, 3, 4], [1e-4], []))!;
  s = advance(s, "e2-lr1e-4", { kind: "done", finalLoss: 1 });
  s = requestStop(s, true);
  assert.equal(isStopping(s), true);
  assert.equal(isFinished(s), false, "the run in flight has not ended yet");
  assert.equal(headline(s), "Stopping after run 2 of 3");
  assert.equal(s.results.length, 1, "the run in flight is not counted as done");

  s = advance(s, "e3-lr1e-4", { kind: "cancelled" });
  assert.equal(s.results[1].note, "Sweep stopped");
  assert.equal(isFinished(s), true);
  assert.equal(isStopping(s), false);
  assert.equal(s.queue.length, 0, "the run still queued will never start");
  assert.equal(headline(s), "Sweep stopped after 2 of 3 runs");
});

test("a stop pressed while a merge cannot be interrupted still ends the sweep once that run finishes", () => {
  let s = begin(planSweep(base, [2, 3, 4], [1e-4], []))!;
  s = requestStop(s, true);
  s = advance(s, "e2-lr1e-4", { kind: "done", finalLoss: 0.4 });
  assert.equal(s.results[0].status, "done", "it finished, so it is recorded as done");
  assert.equal(isFinished(s), true, "and nothing after it starts");
  assert.equal(headline(s), "Sweep stopped after 1 of 3 runs");
});

test("a stop between two runs, with nothing in flight, ends the sweep at once", () => {
  let s = begin(planSweep(base, [2, 3, 4], [1e-4], []))!;
  s = advance(s, "e2-lr1e-4", { kind: "done", finalLoss: 1 });
  s = requestStop(s, false);
  assert.equal(isFinished(s), true, "otherwise it would wait forever for a run that is not coming");
  assert.equal(isStopping(s), false);
  assert.equal(headline(s), "Sweep stopped after 1 of 3 runs");
});

test("a request refused while stopping does not leave the queue behind", () => {
  let s = begin(planSweep(base, [2, 3], [1e-4], []))!;
  s = requestStop(s, true);
  s = refuse(s, "e2-lr1e-4", "no base model");
  assert.equal(isFinished(s), true);
});

test("a request the backend refuses is recorded and does not stall the queue", () => {
  let s = begin(planSweep(base, [2, 3], [1e-4], []))!;
  s = refuse(s, "e2-lr1e-4", "The base model is not downloaded");
  assert.equal(s.results[0].status, "refused");
  assert.equal(s.queue.length, 1);
});

test("the lowest final loss is pointed out, and only among runs that have one", () => {
  const s = begin(planSweep(base, [2, 3], [1e-4], []))!;
  const after = advance(advance(s, "a", { kind: "done", finalLoss: 0.5 }), "b", { kind: "done", finalLoss: 0.3 });
  assert.equal(lowestLoss(after.results), "b");
  assert.equal(lowestLoss([]), null);
  assert.equal(lowestLoss([{ name: "x", status: "failed", finalLoss: null, note: "e" }]), null);
});

test("the header says where a sweep is, and a stopped one does not claim runs it never reached", () => {
  let s = begin(planSweep(base, [2, 3, 4], [1e-4], []))!;
  assert.equal(headline(s), "Sweep, run 1 of 3");
  s = advance(s, "e2-lr1e-4", { kind: "done", finalLoss: 1 });
  assert.equal(headline(s), "Sweep, run 2 of 3");
  s = advance({ ...s, stopped: true }, "e3-lr1e-4", { kind: "cancelled" });
  assert.equal(headline(s), "Sweep stopped after 2 of 3 runs");
  let all = begin(planSweep(base, [2], [1e-4], []))!;
  all = advance(all, "e2-lr1e-4", { kind: "done", finalLoss: 1 });
  assert.equal(headline(all), "Sweep of 1 run");
});
