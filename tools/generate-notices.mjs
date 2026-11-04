#!/usr/bin/env node
// Writes THIRD-PARTY-NOTICES.md: the licenses of everything that ends up inside the app.
//
//   node tools/generate-notices.mjs
//
// Run it from the repository root, after `npm install` and a first `cargo build` (so the
// crates are in ~/.cargo/registry), and again each time a dependency is added or updated.
// It uses only Node and the Cargo that is already installed: no extra tool.
//
// Where the lists come from:
//   - Rust: `cargo tree` for the macOS (Apple Silicon) build, without build and dev tools.
//   - npm: `npm ls --omit=dev`, the packages the interface is built from.
//   - By hand (below): code that is copied into the repository instead of installed.
// The license files are read from the packages themselves. Packages with the same text are
// grouped, so the file stays readable. A few packages ship without their license file: their
// text was copied once from the upstream repository into tools/notices-extra.

import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const OUTPUT = "THIRD-PARTY-NOTICES.md";
const BIG = 512 * 1024 * 1024;

function run(command, cwd) {
  try {
    return execSync(command, { cwd, maxBuffer: BIG, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  } catch (error) {
    // `npm ls` exits with an error code when it finds extraneous packages but still prints the list.
    if (error.stdout) return error.stdout;
    throw error;
  }
}

/** The standard MIT text, for code that comes without its own license file. */
function mitText(copyright) {
  return `MIT License

Copyright (c) ${copyright}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`;
}

/** License, notice and copying files at the top of a package folder. */
function licenseFiles(dir) {
  return readdirSync(dir)
    .filter((name) => /^(licen[cs]e|copying|unlicense|notice)/i.test(name))
    .filter((name) => statSync(join(dir, name)).isFile())
    .sort();
}

// text hash -> { text, packages: Set }
const groups = new Map();
// packages whose folder has no license file: only their license name is known
const withoutText = [];

function addText(text, packageName) {
  const clean = text.replace(/\r\n/g, "\n").trim();
  if (!clean) return;
  const key = createHash("sha256").update(clean).digest("hex");
  if (!groups.has(key)) groups.set(key, { text: clean, packages: new Set() });
  groups.get(key).packages.add(packageName);
}

// Some archives do not contain their license file (it sits in the upstream repository only).
// Their texts were copied once from upstream into tools/notices-extra, named "<group>.<file>".
// `files` maps each license name to the file of that group.
const EXTRA_DIR = "tools/notices-extra";
const extras = [
  {
    match: /^(objc2|block2|dispatch2)/,
    group: "objc2",
    files: { MIT: "LICENSE-MIT.txt", "Apache-2.0": "LICENSE-APACHE.txt", Zlib: "LICENSE-ZLIB.txt" },
  },
  { match: /^alloc-stdlib$/, group: "alloc-stdlib", files: { "BSD-3-Clause": "LICENSE" } },
  {
    match: /^dasp_sample$/,
    group: "dasp_sample",
    files: { MIT: "LICENSE-MIT", "Apache-2.0": "LICENSE-APACHE" },
  },
  {
    match: /^llama-cpp-(2|sys-2)$/,
    group: "llama-cpp-rs",
    files: { MIT: "LICENSE-MIT", "Apache-2.0": "LICENSE-APACHE" },
  },
  // Same text as the LICENSE of `cssparser`, which has the same license and the same upstream.
  { match: /^selectors$/, group: "selectors", files: { "MPL-2.0": "LICENSE" } },
  // The Unlicense is one standard text, copied from the `aho-corasick` crate.
  { match: /^whisper-rs-sys$/, group: "whisper-rs-sys", files: { Unlicense: "UNLICENSE" } },
  { match: /^react-remove-scroll-bar$/, group: "react-remove-scroll-bar", files: { MIT: "LICENSE" } },
];

/** Adds the texts kept in tools/notices-extra for this package. Returns false if it has none. */
function addExtraTexts(name, label, license) {
  const extra = extras.find((e) => e.match.test(name));
  if (!extra) return false;
  const names = (license || "").split(/\s+(?:OR|AND)\s+|\//);
  let added = false;
  for (const id of names) {
    const file = extra.files[id];
    if (!file) continue;
    addText(readFileSync(join(EXTRA_DIR, `${extra.group}.${file}`), "utf8"), label);
    added = true;
  }
  return added;
}

function addPackage({ label, name, dir, license }) {
  const files = licenseFiles(dir);
  if (files.length === 0) {
    if (!addExtraTexts(name, label, license)) {
      withoutText.push(`${label} (${license || "licence non indiquée"})`);
    }
    return;
  }
  for (const file of files) addText(readFileSync(join(dir, file), "utf8"), label);
}

// --- Rust --------------------------------------------------------------------------------

const metadata = JSON.parse(run("cargo metadata --offline --format-version 1", "src-tauri"));
const byKey = new Map(metadata.packages.map((p) => [`${p.name}@${p.version}`, p]));
const crateDir = (p) => p.manifest_path.replace(/\/Cargo\.toml$/, "");

const tree = run(
  'cargo tree --offline -e normal --target aarch64-apple-darwin --prefix none -f "{p}"',
  "src-tauri",
);
const rustPackages = new Map();
for (const line of tree.split("\n")) {
  const found = line.match(/^(\S+) v(\S+)/);
  if (!found) continue;
  const [, name, version] = found;
  if (name === "aede") continue;
  const pkg = byKey.get(`${name}@${version}`);
  if (pkg) rustPackages.set(`${name}@${version}`, pkg);
  else console.error(`Not found in cargo metadata: ${name} ${version}`);
}
for (const pkg of rustPackages.values()) {
  addPackage({
    label: `${pkg.name} ${pkg.version}`,
    name: pkg.name,
    dir: crateDir(pkg),
    license: pkg.license,
  });
}

// --- npm ---------------------------------------------------------------------------------

const npmDirs = run("npm ls --omit=dev --all --parseable", ".").split("\n").filter(Boolean);
const npmPackages = new Map();
for (const dir of npmDirs) {
  let pkg;
  try {
    pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  } catch {
    continue;
  }
  if (!pkg.name || pkg.name === "aede") continue;
  const license = typeof pkg.license === "string" ? pkg.license : pkg.license?.type;
  npmPackages.set(`${pkg.name}@${pkg.version}`, { name: pkg.name, version: pkg.version, dir, license });
}
for (const pkg of npmPackages.values()) {
  addPackage({ label: `${pkg.name} ${pkg.version}`, name: pkg.name, dir: pkg.dir, license: pkg.license });
}

// --- Copied into the repository, or vendored inside a crate ----------------------------------

// Arc UI components are copied in src/components/arc with this notice in their header.
addText(mitText("2026 Elia Kuratli"), "Arc UI (components copied in src/components/arc)");
// Lobe Icons: the OpenAI and Qwen marks of src/setup/logos.tsx. Their license is the standard MIT.
addText(mitText("2023 LobeHub"), "Lobe Icons (logos in src/setup/logos.tsx)");
// whisper.cpp and llama.cpp (with ggml) are compiled from the sources vendored in these crates.
const vendored = [
  ["whisper-rs-sys", "whisper.cpp", "whisper.cpp (with ggml), built by whisper-rs-sys"],
  ["llama-cpp-sys-2", "llama.cpp", "llama.cpp (with ggml), built by llama-cpp-sys-2"],
];
for (const [crate, folder, label] of vendored) {
  const pkg = [...rustPackages.values()].find((p) => p.name === crate);
  if (!pkg) {
    console.error(`Crate ${crate} not found: its vendored license is missing.`);
    process.exitCode = 1;
    continue;
  }
  addText(readFileSync(join(crateDir(pkg), folder, "LICENSE"), "utf8"), label);
}

// --- Report ------------------------------------------------------------------------------

const rustMpl = [...rustPackages.values()].filter((p) => (p.license || "").includes("MPL"));
const sorted = [...groups.values()].sort((a, b) => {
  const first = (g) => [...g.packages].sort()[0];
  return first(a).localeCompare(first(b));
});

const intro = `# Licences des composants tiers

Aède est publié sous licence MIT (voir \`LICENSE\`). Il est construit avec des composants libres écrits par d'autres personnes : leurs licences demandent de garder leur mention de copyright et leur texte de licence avec le logiciel. Ce fichier les rassemble.

Il est généré par \`node tools/generate-notices.mjs\` : ne le modifie pas à la main, relance le script quand une dépendance change.

## Composants principaux

| Composant | Rôle dans Aède | Licence |
|---|---|---|
| [Tauri 2](https://tauri.app) | cadre de l'application, fenêtres, raccourci global | Apache-2.0 ou MIT |
| [whisper.cpp](https://github.com/ggml-org/whisper.cpp) (par ggml), via [whisper-rs](https://codeberg.org/tazz4843/whisper-rs) | transcription de la voix | MIT (whisper-rs : Unlicense) |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) (par ggml), via [llama-cpp-2](https://github.com/utilityai/llama-cpp-rs) | reformulation du texte | MIT |
| [React](https://react.dev) | interface | MIT |
| [Arc UI](https://uiarc.dev) | composants d'interface copiés dans \`src/components/arc\` | MIT |
| [Radix UI](https://www.radix-ui.com) (Select) | liste des micros | MIT |
| [motion](https://motion.dev) et [Lucide](https://lucide.dev) | animations et icônes | MIT et ISC |
| [Geist](https://vercel.com/font) (par Vercel, paquet Fontsource) | police de l'interface | SIL Open Font License 1.1 |
| [Lobe Icons](https://github.com/lobehub/lobe-icons) | logos d'OpenAI et de Qwen de l'écran des modèles | MIT |

Les marques OpenAI et Qwen appartiennent à leurs propriétaires. Elles servent seulement à dire qui a fait chaque modèle.

## Modèles téléchargés

Les modèles ne sont pas livrés avec Aède. Ils sont téléchargés depuis Hugging Face, à la demande de l'utilisateur, au premier lancement.

| Modèle | Auteur | Licence |
|---|---|---|
| Whisper large-v3-turbo, converti au format ggml | OpenAI (conversion : projet whisper.cpp) | MIT |
| Qwen3-4B-Instruct-2507, quantifié Q4_K_M au format GGUF | équipe Qwen, Alibaba Cloud (quantification : Unsloth) | Apache-2.0 |
${
  rustMpl.length > 0
    ? `
## Licence MPL-2.0

${rustMpl.length} paquets Rust sont sous licence MPL-2.0 (${rustMpl
        .map((p) => `\`${p.name}\``)
        .join(", ")}). Aède les utilise sans les modifier ; leur code source est disponible sur leur dépôt public, indiqué dans le registre crates.io.
`
    : ""
}
## Textes de licence

${rustPackages.size} paquets Rust et ${npmPackages.size} paquets npm sont inclus dans l'application. Les paquets qui ont exactement le même texte sont regroupés.
`;

const body = sorted
  .map((group) => {
    const names = [...group.packages].sort();
    return `### ${names.length === 1 ? names[0] : `${names[0]} et ${names.length - 1} autre${names.length > 2 ? "s" : ""}`}

Utilisé par : ${names.join(", ")}.

\`\`\`text
${group.text}
\`\`\`
`;
  })
  .join("\n");

const missing =
  withoutText.length > 0
    ? `
## Paquets sans fichier de licence

Ces paquets n'ont pas de fichier de licence, ni dans leur archive ni dans leur dépôt public. Leur licence, déclarée dans leur manifeste, est indiquée entre parenthèses.

${withoutText.sort().map((line) => `- ${line}`).join("\n")}
`
    : "";

writeFileSync(OUTPUT, `${intro}\n${body}${missing}`);
console.log(
  `${OUTPUT}: ${rustPackages.size} Rust packages, ${npmPackages.size} npm packages, ${sorted.length} distinct texts, ${withoutText.length} without a license file.`,
);
