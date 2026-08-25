import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, relative } from "node:path";

const ignored = new Set([".git", "node_modules", "dist", "target", "target-check"]);
const textExtensions = new Set([
  ".json", ".md", ".mjs", ".ps1", ".rs", ".toml", ".ts", ".tsx", ".yml", ".yaml",
]);
const patterns = [
  { name: "Volcengine access key", regex: /\bAKLT[A-Za-z0-9_-]{12,}\b/g },
  {
    name: "literal credential assignment",
    regex:
      /\b(?:api|access|app)[_-]?key\b\s*[:=]\s*["'][A-Za-z0-9_+\/-]{16,}["']/gi,
  },
];

const findings = [];
function walk(directory) {
  for (const name of readdirSync(directory)) {
    if (ignored.has(name) || name.startsWith(".codex-")) continue;
    const path = join(directory, name);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      walk(path);
    } else if (textExtensions.has(extname(name))) {
      const source = readFileSync(path, "utf8");
      for (const pattern of patterns) {
        for (const match of source.matchAll(pattern.regex)) {
          findings.push({
            file: relative(".", path),
            name: pattern.name,
            sample: match[0].slice(0, 12) + "…",
          });
        }
      }
    }
  }
}

walk(".");
if (findings.length) {
  console.error("Potential committed credentials detected:");
  for (const finding of findings) {
    console.error("- " + finding.file + ": " + finding.name + " (" + finding.sample + ")");
  }
  process.exit(1);
}

console.log("No committed credential patterns detected.");
