import { test } from "node:test";
import assert from "node:assert/strict";
import { nextAction, type Action } from "./snowman.ts";

// Seeded generator (mulberry32) so the draws are reproducible.
function seeded(seed: number): () => number {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

test("walk/run targets stay in range and away from x; durations are positive", () => {
  const rand = seeded(42);
  let x = 0.5, prev: Action | undefined;
  for (let i = 0; i < 5000; i++) {
    const { action, to, ms } = nextAction(rand, x, prev);
    assert.ok(to >= 0.05 && to <= 0.95, `to fuera de rango: ${to}`);
    assert.ok(ms > 0, `ms no positivo: ${ms}`);
    if (action === "walk" || action === "run") {
      assert.ok(Math.abs(to - x) >= 0.15 - 1e-9, `destino demasiado cerca: ${x} → ${to}`);
    } else {
      assert.equal(to, x);
      assert.notEqual(action, prev, "repeats a still action");
      assert.ok(ms >= 800 && ms <= 3000, `duration out of range: ${action} ${ms}`);
    }
    x = to; prev = action;
  }
});

test("every action is reachable", () => {
  const rand = seeded(7);
  const seen = new Set<Action>();
  let x = 0.5;
  for (let i = 0; i < 500; i++) { const n = nextAction(rand, x); seen.add(n.action); x = n.to; }
  assert.deepEqual([...seen].sort(), ["idle", "moon", "run", "slip", "throw", "walk", "wave"]);
});

test("slip lasts as long as its CSS animation and never repeats back to back", () => {
  // 0.95 lands in the last slice of the draw, which is slip.
  const slip = nextAction(() => 0.95, 0.5);
  assert.equal(slip.action, "slip");
  assert.equal(slip.to, 0.5);
  assert.equal(slip.ms, 2000);
  const again = nextAction(() => 0.95, 0.5, "slip");
  assert.equal(again.action, "walk");
});

test("at the edges it still finds a target and running is faster than walking", () => {
  const edge = nextAction(() => 0, 0.95);
  assert.equal(edge.action, "walk");
  assert.ok(edge.to <= 0.8);
  // 0.1 → walk; [0.3, 0.1] → run; same target in both cases.
  const walk = nextAction(() => 0.1, 0.05);
  const seq = [0.3, 0.1];
  const run = nextAction(() => seq.shift()!, 0.05);
  assert.equal(walk.action, "walk");
  assert.equal(run.action, "run");
  assert.equal(run.to, walk.to);
  assert.ok(run.ms < walk.ms);
});

test("out-of-range x is clamped before choosing", () => {
  assert.equal(nextAction(() => 0.5, 2).to, 0.95);
  assert.equal(nextAction(() => 0.5, -1).to, 0.05);
});
