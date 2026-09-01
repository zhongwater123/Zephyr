import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
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
const trackedFiles = execFileSync("git", ["ls-files", "-z"], {
  cwd: root,
  encoding: "buffer",
})
  .toString("utf8")
  .split("\0")
  .filter(Boolean);

for (const file of trackedFiles) {
  if (!textExtensions.has(extname(file)) || file.startsWith(".codex-")) continue;
  const absolute = join(root, file);
  const source = readFileSync(absolute, "utf8");
  for (const pattern of patterns) {
    for (const match of source.matchAll(pattern.regex)) {
      findings.push({
        file: relative(root, absolute),
        name: pattern.name,
        sample: match[0].slice(0, 12) + "…",
      });
    }
  }
}

if (findings.length) {
  console.error("Potential committed credentials detected:");
  for (const finding of findings) {
    console.error("- " + finding.file + ": " + finding.name + " (" + finding.sample + ")");
  }
  process.exit(1);
}

console.log("No committed credential patterns detected.");
