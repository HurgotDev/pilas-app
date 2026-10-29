import { test } from "node:test";
import assert from "node:assert/strict";
import { resolveSkin } from "./skins.ts";

// Meses de Date son base 0: 9 = octubre, 10 = noviembre, 11 = diciembre, 6 = julio.
const at = (month: number, day: number, hour = 12, minute = 0) => new Date(2026, month, day, hour, minute);

test("auto sigue la temporada de Halloween por fecha local", () => {
  assert.equal(resolveSkin("auto", at(9, 14)), "classic");
  assert.equal(resolveSkin("auto", at(9, 15, 0, 0)), "halloween");
  assert.equal(resolveSkin("auto", at(10, 2, 23, 59)), "halloween");
  assert.equal(resolveSkin("auto", at(10, 3)), "classic");
  assert.equal(resolveSkin("auto", at(6, 1)), "classic");
});

test("auto sigue la temporada de Navidad por fecha local", () => {
  assert.equal(resolveSkin("auto", at(10, 30)), "classic");
  assert.equal(resolveSkin("auto", at(11, 1, 0, 0)), "christmas");
  assert.equal(resolveSkin("auto", at(11, 26, 23, 59)), "christmas");
  assert.equal(resolveSkin("auto", at(11, 27)), "classic");
});

test("las opciones fijas se devuelven tal cual sin mirar la fecha", () => {
  assert.equal(resolveSkin("classic", at(9, 31)), "classic");
  assert.equal(resolveSkin("halloween", at(6, 1)), "halloween");
  assert.equal(resolveSkin("christmas", at(6, 1)), "christmas");
});
