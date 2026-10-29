import { test } from "node:test";
import assert from "node:assert/strict";
import { nextAction, type Action } from "./snowman.ts";

// Generador con semilla (mulberry32) para que los sorteos sean reproducibles.
function seeded(seed: number): () => number {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

test("los destinos de walk/run quedan en rango y lejos de x; las duraciones son positivas", () => {
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
      assert.notEqual(action, prev, "repite una acción quieta");
      assert.ok(ms >= 800 && ms <= 3000, `duración fuera de rango: ${action} ${ms}`);
    }
    x = to; prev = action;
  }
});

test("todas las acciones son alcanzables", () => {
  const rand = seeded(7);
  const seen = new Set<Action>();
  let x = 0.5;
  for (let i = 0; i < 500; i++) { const n = nextAction(rand, x); seen.add(n.action); x = n.to; }
  assert.deepEqual([...seen].sort(), ["idle", "moon", "run", "slip", "throw", "walk", "wave"]);
});

test("slip dura lo que su animación CSS y nunca se repite seguido", () => {
  // 0.95 cae en el último tramo del sorteo, que es slip.
  const slip = nextAction(() => 0.95, 0.5);
  assert.equal(slip.action, "slip");
  assert.equal(slip.to, 0.5);
  assert.equal(slip.ms, 2000);
  const again = nextAction(() => 0.95, 0.5, "slip");
  assert.equal(again.action, "walk");
});

test("en los bordes también encuentra destino y correr es más rápido que caminar", () => {
  const edge = nextAction(() => 0, 0.95);
  assert.equal(edge.action, "walk");
  assert.ok(edge.to <= 0.8);
  // 0.1 → walk; [0.3, 0.1] → run; mismo destino en ambos casos.
  const walk = nextAction(() => 0.1, 0.05);
  const seq = [0.3, 0.1];
  const run = nextAction(() => seq.shift()!, 0.05);
  assert.equal(walk.action, "walk");
  assert.equal(run.action, "run");
  assert.equal(run.to, walk.to);
  assert.ok(run.ms < walk.ms);
});

test("x fuera de rango se acota antes de elegir", () => {
  assert.equal(nextAction(() => 0.5, 2).to, 0.95);
  assert.equal(nextAction(() => 0.5, -1).to, 0.05);
});
