import { readFileSync } from "node:fs";

const checks = [
  {
    file: "src/app/AppShell.tsx",
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
    file: "src-tauri/src/voice_controller.rs",
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
  const source = readFileSync(check.file, "utf8");
  for (const token of check.forbidden) {
    if (source.includes(token)) violations.push(check.file + ": " + token);
  }
}

if (violations.length) {
  console.error("ASR layer boundary violations:");
  for (const violation of violations) console.error("- " + violation);
  process.exit(1);
}

console.log("ASR layer boundaries are clean.");
