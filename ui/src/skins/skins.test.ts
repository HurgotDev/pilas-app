import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { inSeason, resolveSkin, SKINS } from "./index.ts";
import { notesFor } from "../sound/sfx.ts";
import type { SkinDef } from "./types.ts";

// Meses de Date son base 0: 9 = octubre, 10 = noviembre, 11 = diciembre, 6 = julio.
const at = (month: number, day: number, hour = 12, minute = 0) => new Date(2026, month, day, hour, minute);
const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

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

test("una temporada que cruza el año contiene el 31 dic y el 1 ene", () => {
  const season = { from: [12, 24], to: [1, 6] } satisfies SkinDef["season"];
  assert.ok(inSeason(season, at(11, 24, 0, 0)));
  assert.ok(inSeason(season, at(11, 31, 23, 59)));
  assert.ok(inSeason(season, new Date(2027, 0, 1)));
  assert.ok(inSeason(season, new Date(2027, 0, 6, 23, 59)));
  assert.ok(!inSeason(season, new Date(2027, 0, 7)));
  assert.ok(!inSeason(season, at(11, 23)));
});

test("las claves de SKINS coinciden con ids.json y cada id con su clave", () => {
  assert.deepEqual(Object.keys(SKINS), JSON.parse(read("./ids.json")));
  for (const [key, def] of Object.entries(SKINS)) assert.equal(def.id, key);
});

test("una skin con solo id y label no rompe a los consumidores puros", () => {
  const bare: SkinDef = { id: "classic", label: "set.skin.classic" };
  assert.equal(resolveSkin(bare.id, at(11, 25)), "classic");
  assert.deepEqual(notesFor(bare, "alerta"), notesFor(SKINS.classic, "alerta"));
  assert.ok(notesFor(bare, "mareo").length > 0);
});

test("los sonidos que una skin no redefine son los clásicos", () => {
  for (const def of Object.values(SKINS) as SkinDef[]) {
    assert.deepEqual(notesFor(def, "recordatorio"), notesFor(SKINS.classic, "recordatorio"));
  }
});

// Selectores de un CSS (sin comentarios), salvo los de dentro de @keyframes; los de @media sí cuentan.
function selectors(css: string): string[] {
  const out: string[] = [];
  const stack: boolean[] = []; // por bloque abierto: ¿es @keyframes?
  let buf = "";
  for (const ch of css.replace(/\/\*[\s\S]*?\*\//g, "")) {
    if (ch === "{") {
      const head = buf.trim();
      const inKeyframes = stack.includes(true);
      if (!inKeyframes && !head.startsWith("@")) out.push(...head.split(",").map((x) => x.trim()));
      stack.push(inKeyframes || head.startsWith("@keyframes"));
      buf = "";
    } else if (ch === "}") {
      stack.pop();
      buf = "";
    } else if (ch === ";") {
      buf = "";
    } else {
      buf += ch;
    }
  }
  return out;
}

test("ninguna skin toca la identidad de la pilita", () => {
  for (const def of Object.values(SKINS) as SkinDef[]) {
    const acc = def.mascot?.accessory ?? "";
    for (const bad of ["<style", 'class="v', "--body", "--shade", "--dark", "--foot", "rig", "data-state"]) {
      assert.ok(!acc.includes(bad), `${def.id}: el accesorio contiene ${bad}`);
    }
    // Todos los CSS de su carpeta: main.ts los carga todos con import.meta.glob.
    for (const file of readdirSync(new URL(`./${def.id}/`, import.meta.url)).filter((f) => f.endsWith(".css"))) {
      const css = read(`./${def.id}/${file}`);
      assert.ok(!css.includes("pilas-mascot"), `${file}: selecciona pilas-mascot`);
      assert.ok(!css.includes(".lit"), `${file}: selecciona .lit`);
      const list = selectors(css);
      assert.ok(list.length > 0);
      for (const sel of list) assert.ok(sel.startsWith(`:root[data-skin="${def.id}"]`), `${file}: selector fuera de su skin: ${sel}`);
    }
  }
});

test("el analizador de selectores ve los de @media y salta los de @keyframes", () => {
  const css = `/* x { } */ a, b { c: d; } @media (x) { e { f: g; } } @keyframes k { from { h: i; } 50% { j: k; } } l { }`;
  assert.deepEqual(selectors(css), ["a", "b", "e", "l"]);
});
