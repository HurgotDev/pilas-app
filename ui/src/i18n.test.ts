import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { errorText, setLang, t, type Key } from "./i18n.ts";

Object.assign(globalThis, { document: { documentElement: { lang: "" } } });

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const errorRs = read("../../src-tauri/src/error.rs");
const source = read("./i18n.ts");
const [enSource, esSource] = source.split("const es");

const backendCodes = [...errorRs.matchAll(/=> "([a-z_]+)"/g)].map((m) => m[1]!).sort();
const keysOf = (src: string) => [...src.matchAll(/"err\.(\w+)":/g)].map((m) => m[1]!).sort();

test("each backend error code has the same err.* keys in en and es", () => {
  assert.ok(backendCodes.length > 0);
  for (const [name, src] of [["en", enSource], ["es", esSource]] as const) {
    assert.deepEqual(keysOf(src), [...backendCodes, "unknown"].sort(), name);
  }
});

test("every backend error code has a non-empty text in both languages", () => {
  for (const lang of ["en", "es"] as const) {
    setLang(lang);
    for (const code of [...backendCodes, "unknown"]) {
      assert.notEqual(t(`err.${code}` as Key), "", `${lang} ${code}`);
    }
  }
  setLang(null);
});

test("a known code is shown in the selected language", () => {
  setLang("en");
  const english = errorText("session_unknown");
  setLang("es");
  const spanish = errorText("session_unknown");
  setLang(null);
  assert.equal(english, "That session has ended.");
  assert.equal(spanish, "Esa sesión ya terminó.");
});

test("unknown input falls back to the generic text and is logged", () => {
  const logged = mock.method(console, "error", () => {});
  try {
    setLang("en");
    const generic = t("err.unknown");
    assert.equal(errorText("no_such_code"), generic);
    assert.equal(errorText(new Error("boom")), generic);
    assert.equal(errorText(undefined), generic);
    assert.equal(logged.mock.callCount(), 3);
    setLang("es");
    assert.equal(errorText("err.unknown"), "Algo salió mal. Inténtalo de nuevo.");
  } finally {
    logged.mock.restore();
    setLang(null);
  }
});
