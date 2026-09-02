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

const isolationRules = [
  {
    file: "src-tauri/src/services.rs",
    forbidden: [
      "WindowsCredentialStore",
      "WindowsNativeConfirmation",
      '#[cfg(target_os = "',
    ],
  },
  {
    file: "src-tauri/src/physical_shortcut.rs",
    forbidden: ["use windows::", "MapVirtualKeyW", "VK_SPACE"],
  },
  {
    file: "src-tauri/src/voice_input_service.rs",
    forbidden: ["target_os", "Windows", "MacOS", "macOS"],
  },
  {
    file: "src-tauri/src/shortcut_manager/mod.rs",
    forbidden: ["windows_keyboard", "WindowsKeyboardEngine", "target_os"],
  },
  {
    file: "src-tauri/src/desktop_support.rs",
    forbidden: ["UiVoiceInput", "uiVoiceInput"],
  },
];

for (const { file, forbidden } of isolationRules) {
  const source = readFileSync(file, "utf8");
  for (const token of forbidden) {
    if (source.includes(token)) violations.push(`${file}: ${token}`);
  }
}

const platformModule = readFileSync("src-tauri/src/platform.rs", "utf8");
for (const expected of ['mod windows;', 'mod macos;', "DesktopSupportPolicy"]) {
  if (!platformModule.includes(expected)) {
    violations.push(`src-tauri/src/platform.rs: missing ${expected}`);
  }
}

function readJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

const baseConfig = readJson("src-tauri/tauri.conf.json");
const windowsConfig = readJson("src-tauri/tauri.windows.conf.json");
const macosConfig = readJson("src-tauri/tauri.macos.conf.json");
if (baseConfig.bundle?.externalBin !== undefined) {
  violations.push("src-tauri/tauri.conf.json: shared config must not register a sidecar");
}
if (
  windowsConfig.bundle?.externalBin?.length !== 1 ||
  windowsConfig.bundle.externalBin[0] !== "binaries/zephyr-paste-helper"
) {
  violations.push("src-tauri/tauri.windows.conf.json: missing Windows paste helper");
}
if (
  macosConfig.bundle?.externalBin?.length !== 0 ||
  macosConfig.bundle?.targets?.length !== 1 ||
  macosConfig.bundle.targets[0] !== "app" ||
  macosConfig.bundle?.macOS?.minimumSystemVersion !== "15.0" ||
  macosConfig.bundle?.macOS?.infoPlist !== "Info.plist"
) {
  violations.push("src-tauri/tauri.macos.conf.json: invalid phase-two bundle contract");
}

const infoPlist = readFileSync("src-tauri/Info.plist", "utf8");
if (!infoPlist.includes("NSMicrophoneUsageDescription")) {
  violations.push("src-tauri/Info.plist: missing microphone usage description");
}

const ciWorkflow = readFileSync(".github/workflows/ci.yml", "utf8");
for (const expected of [
  'MACOSX_DEPLOYMENT_TARGET: "15.0"',
  'lipo -archs "$EXECUTABLE_PATH"',
  'MINIMUM_SYSTEM_VERSION',
  'NSMicrophoneUsageDescription',
  "'*paste-helper*'",
  'codesign --verify --deep --strict',
  'kill -0 "$APP_PID"',
  "npm run package:windows:check",
]) {
  if (!ciWorkflow.includes(expected)) {
    violations.push(`.github/workflows/ci.yml: missing ${expected}`);
  }
}
if (ciWorkflow.includes("macos-15-intel")) {
  violations.push(".github/workflows/ci.yml: Intel probe is outside phase-two scope");
}
for (const forbidden of ["NSAppleEventsUsageDescription", "Accessibility"]) {
  if (infoPlist.includes(forbidden)) {
    violations.push(`src-tauri/Info.plist: forbidden premature permission ${forbidden}`);
  }
}

const cargoManifest = readFileSync("src-tauri/Cargo.toml", "utf8");
for (const expected of [
  '[target.\'cfg(target_os = "windows")\'.dependencies]',
  'features = ["windows-native"]',
  '[target.\'cfg(target_os = "macos")\'.dependencies]',
  'features = ["apple-native"]',
]) {
  if (!cargoManifest.includes(expected)) {
    violations.push(`src-tauri/Cargo.toml: missing ${expected}`);
  }
}

if (violations.length) {
  console.error("Shared platform boundary violations:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}

console.log("Shared platform boundaries are clean.");
