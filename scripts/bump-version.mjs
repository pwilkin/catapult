#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("Usage: node scripts/bump-version.mjs <semver>");
  console.error("Example: node scripts/bump-version.mjs 0.1.6");
  process.exit(1);
}

function updateJson(relPath, mutate) {
  const path = resolve(root, relPath);
  const text = readFileSync(path, "utf8");
  const data = JSON.parse(text);
  const before = JSON.stringify(data);
  mutate(data);
  if (JSON.stringify(data) === before) {
    console.log(`  ${relPath}: already at target`);
    return;
  }
  const trailingNewline = text.endsWith("\n") ? "\n" : "";
  writeFileSync(path, JSON.stringify(data, null, 2) + trailingNewline);
  console.log(`  ${relPath}: updated`);
}

function updateCargoToml(relPath) {
  const path = resolve(root, relPath);
  const text = readFileSync(path, "utf8");
  const re = /(\[package\][\s\S]*?\n\s*version\s*=\s*")([^"]+)(")/;
  const match = text.match(re);
  if (!match) {
    console.error(`  ${relPath}: could not find [package].version`);
    process.exit(1);
  }
  if (match[2] === version) {
    console.log(`  ${relPath}: already at target`);
    return;
  }
  writeFileSync(path, text.replace(re, `$1${version}$3`));
  console.log(`  ${relPath}: updated`);
}

console.log(`Bumping version to ${version}`);

updateJson("package.json", (p) => {
  p.version = version;
});

updateJson("package-lock.json", (p) => {
  p.version = version;
  if (p.packages && p.packages[""]) {
    p.packages[""].version = version;
  }
});

updateJson("src-tauri/tauri.conf.json", (p) => {
  p.version = version;
});

updateCargoToml("src-tauri/Cargo.toml");

console.log("Done. Don't forget to update CHANGELOG.md and commit.");
