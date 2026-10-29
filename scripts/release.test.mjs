import assert from "node:assert/strict";
import { test } from "node:test";
import {
  extractSection,
  nextVersion,
  parseFragment,
  prependSection,
  releaseVersion,
  renderSection,
  setCargoVersion,
  buildLatest,
  renderCask,
  setJsonVersion,
} from "./release.mjs";

test("parseFragment reads bump (with comment) and body", () => {
  const f = parseFragment("---\nbump: minor   # patch | minor | major\n---\nAñade algo.\n  Y más.\n");
  assert.deepEqual(f, { bump: "minor", body: "Añade algo. Y más." });
});

test("parseFragment rejects invalid fragments", () => {
  assert.throws(() => parseFragment("Sin frontmatter"), /frontmatter/);
  assert.throws(() => parseFragment("---\nfoo: 1\n---\nx"), /bump:/);
  assert.throws(() => parseFragment("---\nbump: huge\n---\nx"), /invalid bump/);
  assert.throws(() => parseFragment("---\nbump: patch\n---\n  \n"), /is empty/);
});

test("parseFragment accepts bump none and lists it in the error", () => {
  assert.deepEqual(parseFragment("---\nbump: none\n---\nAdd a GIF.\n"), { bump: "none", body: "Add a GIF." });
  assert.throws(() => parseFragment("---\nbump: huge\n---\nx"), /use none \| patch \| minor \| major/);
});

test("releaseVersion ignores none fragments and returns null when all are none", () => {
  assert.equal(releaseVersion("0.3.0", [{ bump: "none" }]), null);
  assert.equal(releaseVersion("0.3.0", [{ bump: "none" }, { bump: "none" }]), null);
  assert.equal(releaseVersion("0.3.0", [{ bump: "none" }, { bump: "patch" }]), "0.3.1");
  assert.equal(releaseVersion("0.3.0", [{ bump: "minor" }, { bump: "none" }]), "0.4.0");
});

test("renderSection leaves out none fragments", () => {
  const s = renderSection("0.3.1", "2026-10-29", [
    { bump: "none", body: "Add a GIF." },
    { bump: "patch", body: "Fix A." },
  ]);
  assert.equal(s, "## [0.3.1] - 2026-10-29\n\n### Fixes and improvements\n\n- Fix A.\n");
});

test("nextVersion applies the highest bump", () => {
  assert.equal(nextVersion("0.1.0", ["patch"]), "0.1.1");
  assert.equal(nextVersion("0.1.3", ["patch", "minor"]), "0.2.0");
  assert.equal(nextVersion("0.4.2", ["minor", "major"]), "1.0.0");
  assert.equal(nextVersion("1.2.3", ["major"]), "2.0.0");
  assert.throws(() => nextVersion("0.1.0", []), /no fragments/);
  assert.throws(() => nextVersion("v1", ["patch"]), /invalid current version/);
});

test("renderSection groups by bump and omits empty groups", () => {
  const s = renderSection("0.2.0", "2026-10-28", [
    { bump: "patch", body: "Fix A." },
    { bump: "minor", body: "Add B." },
    { bump: "patch", body: "Fix C." },
  ]);
  assert.equal(
    s,
    "## [0.2.0] - 2026-10-28\n\n### Added\n\n- Add B.\n\n### Fixes and improvements\n\n- Fix A.\n- Fix C.\n",
  );
});

test("prependSection inserts before the latest version and extractSection reads it back", () => {
  const base = "# Changelog\n\nIntro.\n\n## [0.1.0] - 2026-01-01\n\n### Added\n\n- First.\n";
  const out = prependSection(base, renderSection("0.1.1", "2026-02-02", [{ bump: "patch", body: "Fix." }]));
  assert.ok(out.startsWith("# Changelog\n\nIntro.\n\n## [0.1.1] - 2026-02-02\n"));
  assert.equal(extractSection(out, "0.1.1"), "### Fixes and improvements\n\n- Fix.");
  assert.equal(extractSection(out, "0.1.0"), "### Added\n\n- First.");
  assert.equal(extractSection(out, "9.9.9"), null);
  assert.ok(prependSection("# Changelog\n", "## [1.0.0]\n").endsWith("# Changelog\n\n## [1.0.0]\n"));
});

test("setJsonVersion and setCargoVersion only touch the package version", () => {
  assert.equal(setJsonVersion('{\n  "version": "0.1.0",\n  "x": {"version": "9"}\n}', "0.2.0"),
    '{\n  "version": "0.2.0",\n  "x": {"version": "9"}\n}');
  const cargo = '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.1.0"\n\n[workspace.dependencies]\nserde = { version = "1" }\n';
  assert.equal(setCargoVersion(cargo, "0.2.0"), cargo.replace('"0.1.0"', '"0.2.0"'));
  assert.throws(() => setCargoVersion("[package]\nversion = \"1\"\n", "2.0.0"), /workspace\.package/);
});

test("buildLatest points both platforms to the same tarball", () => {
  const latest = buildLatest({
    version: "1.2.3",
    notes: "- Change.",
    pubDate: "2026-10-28T10:00:00.000Z",
    signature: "c2ln",
    url: "https://example.com/Pilas_universal.app.tar.gz",
  });
  assert.equal(latest.version, "1.2.3");
  assert.equal(latest.pub_date, "2026-10-28T10:00:00.000Z");
  assert.deepEqual(Object.keys(latest.platforms).sort(), ["darwin-aarch64", "darwin-x86_64"]);
  assert.deepEqual(latest.platforms["darwin-aarch64"], latest.platforms["darwin-x86_64"]);
  assert.equal(latest.platforms["darwin-x86_64"].signature, "c2ln");
});

test("renderCask fills version and sha256 and validates both", () => {
  const sha = "a".repeat(64);
  const out = renderCask('version "@VERSION@"\nsha256 "@SHA256@"', { version: "1.2.3", sha256: sha });
  assert.equal(out, `version "1.2.3"\nsha256 "${sha}"`);
  assert.throws(() => renderCask("@VERSION@", { version: "1.2", sha256: sha }), /invalid version/);
  assert.throws(() => renderCask("@SHA256@", { version: "1.2.3", sha256: "xyz" }), /sha256/);
});
