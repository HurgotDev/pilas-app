import assert from "node:assert/strict";
import { test } from "node:test";
import {
  extractSection,
  nextVersion,
  parseFragment,
  prependSection,
  renderSection,
  setCargoVersion,
  setJsonVersion,
} from "./release.mjs";

test("parseFragment lee bump (con comentario) y cuerpo", () => {
  const f = parseFragment("---\nbump: minor   # patch | minor | major\n---\nAñade algo.\n  Y más.\n");
  assert.deepEqual(f, { bump: "minor", body: "Añade algo. Y más." });
});

test("parseFragment rechaza fragmentos inválidos", () => {
  assert.throws(() => parseFragment("Sin frontmatter"), /frontmatter/);
  assert.throws(() => parseFragment("---\nfoo: 1\n---\nx"), /bump:/);
  assert.throws(() => parseFragment("---\nbump: huge\n---\nx"), /inválido/);
  assert.throws(() => parseFragment("---\nbump: patch\n---\n  \n"), /vacío/);
});

test("nextVersion aplica el bump más alto", () => {
  assert.equal(nextVersion("0.1.0", ["patch"]), "0.1.1");
  assert.equal(nextVersion("0.1.3", ["patch", "minor"]), "0.2.0");
  assert.equal(nextVersion("0.4.2", ["minor", "major"]), "1.0.0");
  assert.equal(nextVersion("1.2.3", ["major"]), "2.0.0");
  assert.throws(() => nextVersion("0.1.0", []), /fragmentos/);
  assert.throws(() => nextVersion("v1", ["patch"]), /inválida/);
});

test("renderSection agrupa por tipo y omite grupos vacíos", () => {
  const s = renderSection("0.2.0", "2026-10-28", [
    { bump: "patch", body: "Arregla A." },
    { bump: "minor", body: "Añade B." },
    { bump: "patch", body: "Arregla C." },
  ]);
  assert.equal(
    s,
    "## [0.2.0] - 2026-10-28\n\n### Novedades\n\n- Añade B.\n\n### Correcciones\n\n- Arregla A.\n- Arregla C.\n",
  );
});

test("prependSection inserta antes de la última versión y extractSection la recupera", () => {
  const base = "# Changelog\n\nIntro.\n\n## [0.1.0] - 2026-01-01\n\n### Novedades\n\n- Primera.\n";
  const out = prependSection(base, renderSection("0.1.1", "2026-02-02", [{ bump: "patch", body: "Fix." }]));
  assert.ok(out.startsWith("# Changelog\n\nIntro.\n\n## [0.1.1] - 2026-02-02\n"));
  assert.equal(extractSection(out, "0.1.1"), "### Correcciones\n\n- Fix.");
  assert.equal(extractSection(out, "0.1.0"), "### Novedades\n\n- Primera.");
  assert.equal(extractSection(out, "9.9.9"), null);
  assert.ok(prependSection("# Changelog\n", "## [1.0.0]\n").endsWith("# Changelog\n\n## [1.0.0]\n"));
});

test("setJsonVersion y setCargoVersion solo tocan la versión del paquete", () => {
  assert.equal(setJsonVersion('{\n  "version": "0.1.0",\n  "x": {"version": "9"}\n}', "0.2.0"),
    '{\n  "version": "0.2.0",\n  "x": {"version": "9"}\n}');
  const cargo = '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.1.0"\n\n[workspace.dependencies]\nserde = { version = "1" }\n';
  assert.equal(setCargoVersion(cargo, "0.2.0"), cargo.replace('"0.1.0"', '"0.2.0"'));
  assert.throws(() => setCargoVersion("[package]\nversion = \"1\"\n", "2.0.0"), /workspace\.package/);
});
