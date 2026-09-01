import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const sharedFiles = [
  "src-tauri/src/delivery.rs",
  "src-tauri/src/inject.rs",
  "src-tauri/src/pending_output_service.rs",
  "src-tauri/src/target.rs",
  "src-tauri/src/target_port.rs",
];
const voiceFiles = readdirSync("src-tauri/src/voice_controller", {
  recursive: true,
  withFileTypes: true,
})
  .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
  .map((entry) => path.join(entry.parentPath, entry.name));

const forbiddenNativeBoundary = [
  "windows::",
  "HWND",
  "TargetWindowIdentity",
  "capture_foreground_target",
  "validate_target_exists",
  "validate_foreground_target",
  "activate_target",
  '#[cfg(target_os = "',
];

const forbiddenWorkflowAccess = ["payload_as::<", "CapturedTarget::new"];

const violations = [];
for (const file of [...sharedFiles, ...voiceFiles]) {
  const source = readFileSync(file, "utf8");
  for (const token of forbiddenNativeBoundary) {
    if (source.includes(token)) violations.push(`${file}: ${token}`);
  }
  if (file !== "src-tauri/src/target_port.rs") {
    for (const token of forbiddenWorkflowAccess) {
      if (source.includes(token)) violations.push(`${file}: ${token}`);
    }
  }
}

if (violations.length) {
  console.error("Shared platform boundary violations:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}

console.log("Shared platform boundaries are clean.");
