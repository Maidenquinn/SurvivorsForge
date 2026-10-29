#!/usr/bin/env node
// Dependency-free catalog parity and template reference check.
// Run from repository root: node scripts/check-i18n.mjs
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const root = process.cwd();
const source = fs.readFileSync(path.join(root, "crates/forge-web/static/i18n.js"), "utf8");
const catalogMatch = source.match(/var translations = ([\s\S]*?);\s*var supported/);
if (!catalogMatch) throw new Error("Cannot locate translation catalog");
const catalog = JSON.parse(catalogMatch[1]);
const expected = ["en","pl","cs","de","pt-BR","es","fr","ro","tr","ru","uk","zh-Hans","ja","ko"];
const actual = Object.keys(catalog);
if (JSON.stringify(actual.sort()) !== JSON.stringify([...expected].sort())) throw new Error("Locale list does not match the game's 14 languages");
const keys = Object.keys(catalog.en).sort();
for (const locale of expected) {
  const entries = catalog[locale];
  if (JSON.stringify(Object.keys(entries).sort()) !== JSON.stringify(keys)) throw new Error("Missing/extra translation keys for " + locale);
  for (const [key, value] of Object.entries(entries)) {
    if (typeof value !== "string" || !value.trim()) throw new Error("Empty translation: " + locale + "." + key);
  }
}
const templates = path.join(root, "crates/forge-web/templates");
const unknown = [];
for (const filename of fs.readdirSync(templates).filter(name => name.endsWith(".html"))) {
  const html = fs.readFileSync(path.join(templates, filename), "utf8");
  for (const match of html.matchAll(/data-i18n(?:-placeholder|-confirm|-aria-label)?="([^"]+)"/g)) {
    if (!(match[1] in catalog.en)) unknown.push(filename + ": " + match[1]);
  }
}
if (unknown.length) throw new Error("Unknown translation keys:\n" + unknown.join("\n"));
vm.runInNewContext(source, {
  document: {addEventListener() {}},
  window: {},
  navigator: {language:"en"},
  localStorage: {getItem() {return null}},
});
console.log("i18n OK:", expected.length, "languages,", keys.length, "keys per language, all template references valid");
