#!/usr/bin/env node
// Herramienta de releases: fragmentos de changelog en .changes/, versión y notas.
// Usage: node scripts/release.mjs <prepare|check|notes <version>|version|
//   latest <version> <signature.sig>|cask <version> <dmg>>
import { execFileSync, spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const BUMPS = ["patch", "minor", "major"];
const GROUPS = { major: "Breaking changes", minor: "Added", patch: "Fixes and improvements" };

// Devuelve { bump, body } o lanza un Error con el motivo.
export function parseFragment(text) {
  const m = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/.exec(text);
  if (!m) throw new Error("missing frontmatter (--- bump: ... ---)");
  const bumpLine = /^bump:\s*(\S+)\s*(#.*)?$/m.exec(m[1]);
  if (!bumpLine) throw new Error("missing 'bump:' in the frontmatter");
  const bump = bumpLine[1];
  if (!BUMPS.includes(bump)) throw new Error(`invalid bump '${bump}' (use ${BUMPS.join(" | ")})`);
  // El cuerpo se convierte en una sola viñeta.
  const body = m[2].trim().replace(/\s*\n\s*/g, " ");
  if (!body) throw new Error("the body is empty");
  return { bump, body };
}

export function nextVersion(current, bumps) {
  const parts = /^(\d+)\.(\d+)\.(\d+)$/.exec(current);
  if (!parts) throw new Error(`invalid current version: ${current}`);
  let [major, minor, patch] = parts.slice(1).map(Number);
  const top = Math.max(...bumps.map((b) => BUMPS.indexOf(b)));
  if (top < 0) throw new Error("there are no fragments");
  // Se aplica literalmente aunque major sea 0: major lleva a 1.0.0.
  if (top === 2) [major, minor, patch] = [major + 1, 0, 0];
  else if (top === 1) [minor, patch] = [minor + 1, 0];
  else patch += 1;
  return `${major}.${minor}.${patch}`;
}

export function renderSection(version, date, fragments) {
  let out = `## [${version}] - ${date}\n`;
  for (const bump of ["major", "minor", "patch"]) {
    const items = fragments.filter((f) => f.bump === bump);
    if (items.length) out += `\n### ${GROUPS[bump]}\n\n${items.map((f) => `- ${f.body}`).join("\n")}\n`;
  }
  return out;
}

// Inserta la sección antes de la primera versión existente (o al final).
export function prependSection(changelog, section) {
  const i = changelog.search(/^## \[/m);
  if (i < 0) return `${changelog.trimEnd()}\n\n${section}`;
  return `${changelog.slice(0, i)}${section}\n${changelog.slice(i)}`;
}

// Cuerpo de la sección de una versión, sin su título; null si no existe.
export function extractSection(changelog, version) {
  const lines = changelog.split("\n");
  const start = lines.findIndex((l) => l.startsWith(`## [${version}]`));
  if (start < 0) return null;
  let end = lines.findIndex((l, i) => i > start && l.startsWith("## ["));
  if (end < 0) end = lines.length;
  return lines.slice(start + 1, end).join("\n").trim();
}

// Cambia la primera aparición de "version": "..." sin reformatear el JSON.
export function setJsonVersion(text, version) {
  return text.replace(/"version":\s*"[^"]*"/, `"version": "${version}"`);
}

export function setCargoVersion(text, version) {
  const re = /(\[workspace\.package\][^[]*?^version\s*=\s*")[^"]*"/m;
  if (!re.test(text)) throw new Error("version not found in [workspace.package] of Cargo.toml");
  return text.replace(re, `$1${version}"`);
}

// latest.json del updater (formato estático de Tauri). Ambas plataformas
// apuntan al mismo tarball universal; la firma es el contenido del .sig.
export function buildLatest({ version, notes, pubDate, signature, url }) {
  const platform = { signature, url };
  return {
    version,
    notes,
    pub_date: pubDate,
    platforms: { "darwin-aarch64": platform, "darwin-x86_64": platform },
  };
}

// Rellena la plantilla del cask. La versión entra en Ruby: se valida antes.
export function renderCask(template, { version, sha256 }) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`invalid version: ${version}`);
  if (!/^[0-9a-f]{64}$/.test(sha256)) throw new Error("invalid sha256");
  return template.replaceAll("@VERSION@", version).replaceAll("@SHA256@", sha256);
}

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const CHANGES = path.join(ROOT, ".changes");
const file = (p) => path.join(ROOT, p);
const read = (p) => fs.readFileSync(file(p), "utf8");
const write = (p, s) => fs.writeFileSync(file(p), s);

function readFragments() {
  const names = fs.existsSync(CHANGES)
    ? fs.readdirSync(CHANGES).filter((n) => n.endsWith(".md") && n !== "README.md").sort()
    : [];
  const errors = [];
  const fragments = [];
  for (const name of names) {
    try {
      fragments.push({ name, ...parseFragment(fs.readFileSync(path.join(CHANGES, name), "utf8")) });
    } catch (e) {
      errors.push(`.changes/${name}: ${e.message}`);
    }
  }
  return { fragments, errors };
}

const currentVersion = () => JSON.parse(read("src-tauri/tauri.conf.json")).version;

function today() {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function cargoUpdate() {
  const opts = { cwd: ROOT, stdio: "inherit" };
  if (spawnSync("cargo", ["update", "-w", "--offline"], opts).status === 0) return;
  execFileSync("cargo", ["update", "-w"], opts);
}

function prepare() {
  const { fragments, errors } = readFragments();
  if (errors.length) throw new Error(errors.join("\n"));
  if (!fragments.length) throw new Error("there are no fragments in .changes/: nothing to publish");
  const version = nextVersion(currentVersion(), fragments.map((f) => f.bump));

  write("src-tauri/tauri.conf.json", setJsonVersion(read("src-tauri/tauri.conf.json"), version));
  write("package.json", setJsonVersion(read("package.json"), version));
  const lock = JSON.parse(read("package-lock.json"));
  lock.version = version;
  lock.packages[""].version = version;
  write("package-lock.json", `${JSON.stringify(lock, null, 2)}\n`);
  write("Cargo.toml", setCargoVersion(read("Cargo.toml"), version));
  cargoUpdate();

  const changelog = fs.existsSync(file("CHANGELOG.md")) ? read("CHANGELOG.md") : "# Changelog\n";
  write("CHANGELOG.md", prependSection(changelog, renderSection(version, today(), fragments)));
  for (const f of fragments) fs.rmSync(path.join(CHANGES, f.name));
  console.log(version);
}

function changelogSection(version) {
  const changelog = fs.existsSync(file("CHANGELOG.md")) ? read("CHANGELOG.md") : "";
  const body = extractSection(changelog, version);
  if (body === null) throw new Error(`CHANGELOG.md has no section for ${version}`);
  return body;
}

function main([cmd, ...args]) {
  if (cmd === "prepare") return prepare();
  if (cmd === "version") return console.log(currentVersion());
  if (cmd === "check") {
    const { fragments, errors } = readFragments();
    if (errors.length) throw new Error(errors.join("\n"));
    return console.log(`${fragments.length} valid fragment(s)`);
  }
  if (cmd === "notes") {
    if (!args[0]) throw new Error("usage: release.mjs notes <version>");
    return console.log(changelogSection(args[0]));
  }
  if (cmd === "latest") {
    const [version, sigPath] = args;
    if (!version || !sigPath) throw new Error("usage: release.mjs latest <version> <signature.sig>");
    const signature = fs.readFileSync(sigPath, "utf8").trim();
    if (!signature) throw new Error(`empty signature: ${sigPath}`);
    const url = `https://github.com/HurgotDev/pilas-app/releases/download/v${version}/Pilas_universal.app.tar.gz`;
    const pubDate = new Date().toISOString();
    const notes = changelogSection(version);
    return console.log(JSON.stringify(buildLatest({ version, notes, pubDate, signature, url }), null, 2));
  }
  if (cmd === "cask") {
    const [version, dmg] = args;
    if (!version || !dmg) throw new Error("usage: release.mjs cask <version> <dmg>");
    const sha256 = crypto.createHash("sha256").update(fs.readFileSync(dmg)).digest("hex");
    const template = read("packaging/homebrew/pilas.rb.tmpl");
    return console.log(renderCask(template, { version, sha256 }));
  }
  throw new Error("usage: release.mjs <prepare|check|notes <version>|version|latest <version> <signature.sig>|cask <version> <dmg>>");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }
}
