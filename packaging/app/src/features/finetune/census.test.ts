import { test } from "node:test";
import assert from "node:assert/strict";
import { censusVerdict } from "./census.ts";
import type { StepRecord } from "../../lib/types.ts";

const step = (grad_norm: number | null, skipped: StepRecord["skipped"] = null): StepRecord => ({
  epoch: 1,
  step: 1,
  loss: 2,
  grad_norm,
  skipped,
  t_ms: 0,
});

test("nothing to judge before the first step", () => {
  assert.equal(censusVerdict([]), null);
});

test("norms of tens to hundreds with nothing skipped are healthy", () => {
  const v = censusVerdict([step(4.9), step(61), step(149)])!;
  assert.equal(v.level, "ok");
  assert.match(v.detail, /4\.9 to 149\.0/);
});

test("a norm at 1e6 or more means the backward pass is broken", () => {
  const v = censusVerdict([step(50), step(2.5e29)])!;
  assert.equal(v.level, "broken");
  assert.match(v.detail, /2\.5e\+29/);
});

test("skipped steps are a warning and say how many", () => {
  const v = censusVerdict([step(40), step(null, "no_masked_tokens"), step(55)])!;
  assert.equal(v.level, "warn");
  assert.match(v.headline, /1 of 3 steps were skipped/);
});

test("high but finite norms warn", () => {
  assert.equal(censusVerdict([step(900), step(700)])!.level, "warn");
});

test("a run where every step was skipped has no usable gradients", () => {
  const v = censusVerdict([step(null, "non_finite_loss"), step(null, "bad_grad_norm")])!;
  assert.equal(v.level, "broken");
  assert.match(v.headline, /No usable gradients/);
});

test("broken wins over skipped", () => {
  assert.equal(censusVerdict([step(1e7), step(null, "bad_grad_norm")])!.level, "broken");
});

test("steps that ran without a usable norm are not reported as skipped", () => {
  const v = censusVerdict([step(null), step(Number.NaN)])!;
  assert.equal(v.level, "broken");
  assert.doesNotMatch(v.detail, /were skipped/);
  assert.match(v.detail, /ran, but none reported a finite gradient norm/);
});

test("when every step was skipped, it says so", () => {
  const v = censusVerdict([step(null, "non_finite_loss"), step(null, "bad_grad_norm")])!;
  assert.match(v.detail, /All 2 steps were skipped/);
});
