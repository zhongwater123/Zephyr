import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const checks = [
  {
    file: "src/app/AppShellV2.tsx",
    forbidden: [
      "enable_punc",
      "enable_itn",
      "enable_ddc",
      "enable_accelerate_text",
      "resource_id",
      "openspeech",
      "recognition_behavior",
    ],
  },
  {
    file: "src-tauri/src/provider.rs",
    forbidden: [
      "enable_punc",
      "resource_id",
      "openspeech",
      "45000002",
    ],
  },
  {
    directory: "src-tauri/src/voice_controller",
    forbidden: [
      "45000002",
      "empty audio",
      "enable_punc",
      "resource_id",
      "openspeech",
    ],
  },
];

const violations = [];
for (const check of checks) {
  const files = check.directory
    ? readdirSync(check.directory, { recursive: true, withFileTypes: true })
        .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
        .map((entry) => path.join(entry.parentPath, entry.name))
    : [check.file];
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    for (const token of check.forbidden) {
      if (source.includes(token)) violations.push(file + ": " + token);
    }
  }
}

if (violations.length) {
  console.error("ASR layer boundary violations:");
  for (const violation of violations) console.error("- " + violation);
  process.exit(1);
}

console.log("ASR layer boundaries are clean.");
