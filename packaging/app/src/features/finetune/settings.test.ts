import { test } from "node:test";
import assert from "node:assert/strict";
import { loraProblem, runProblem } from "./settings.ts";

test("the usual LoRA settings are fine", () => {
  assert.equal(loraProblem("8", "16", "512"), null);
  assert.equal(loraProblem("256", "0.5", "8192"), null);
  assert.equal(loraProblem(" 8 ", "16", "16"), null);
});

test("each LoRA setting names itself when it is out of range or not a number", () => {
  assert.match(loraProblem("0", "16", "512")!, /rank/);
  assert.match(loraProblem("257", "16", "512")!, /rank/);
  assert.match(loraProblem("8.5", "16", "512")!, /rank/, "the backend takes a whole number");
  assert.match(loraProblem("", "16", "512")!, /rank/);
  assert.match(loraProblem("8", "0", "512")!, /alpha/);
  assert.match(loraProblem("8", "-1", "512")!, /alpha/);
  assert.match(loraProblem("8", "abc", "512")!, /alpha/);
  assert.match(loraProblem("8", "16", "15")!, /Max sequence/);
  assert.match(loraProblem("8", "16", "8193")!, /Max sequence/);
  assert.match(loraProblem("8", "16", "512.5")!, /Max sequence/);
});

test("the first bad LoRA setting is the one reported", () => {
  assert.match(loraProblem("0", "0", "0")!, /rank/);
});

test("a training run's learning rate and epochs are checked the way the backend checks them", () => {
  assert.equal(runProblem("1e-4", "2"), null);
  assert.equal(runProblem("1", "100"), null);
  assert.match(runProblem("0", "2")!, /Learning rate/);
  assert.match(runProblem("1.5", "2")!, /Learning rate/);
  assert.match(runProblem("fast", "2")!, /Learning rate/);
  assert.match(runProblem("1e-4", "0")!, /Epochs/);
  assert.match(runProblem("1e-4", "101")!, /Epochs/);
  assert.match(runProblem("1e-4", "2.5")!, /Epochs/);
});
