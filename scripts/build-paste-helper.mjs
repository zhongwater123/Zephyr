import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, copyFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { loadDeploymentEnvironment } from "./deployment-env.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDirectory, "..");
const targetTriple = "x86_64-pc-windows-msvc";
const release = process.argv.slice(2).includes("--release");
const profile = release ? "release" : "debug";
const environment = loadDeploymentEnvironment(projectRoot);
const targetDirectory = resolve(
  environment.GY_TYPING_CARGO_TARGET_DIR ||
    resolve(projectRoot, "src-tauri", "target"),
);
environment.CARGO_TARGET_DIR = targetDirectory;

const result = spawnSync(
  environment.CARGO || "cargo",
  [
    "build",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--package",
    "zephyr-paste-helper",
    "--target",
    targetTriple,
    ...(release ? ["--release"] : []),
  ],
  {
    cwd: projectRoot,
    env: environment,
    stdio: "inherit",
    windowsHide: true,
  },
);

if (result.error) {
  throw new Error(`无法启动 paste helper 构建：${result.error.message}`);
}
if (result.status !== 0) {
  throw new Error(`paste helper 构建失败（退出码 ${result.status ?? "unknown"}）`);
}

const source = resolve(
  targetDirectory,
  targetTriple,
  profile,
  "zephyr-paste-helper.exe",
);
if (!existsSync(source)) {
  throw new Error(`paste helper 构建成功但没有找到产物：${source}`);
}

const binary = readFileSync(source);
if (binary.length < 0x40 || binary.toString("ascii", 0, 2) !== "MZ") {
  throw new Error("paste helper 不是有效的 Windows PE 文件");
}
const peOffset = binary.readUInt32LE(0x3c);
if (
  peOffset + 6 > binary.length ||
  binary.toString("binary", peOffset, peOffset + 4) !== "PE\0\0" ||
  binary.readUInt16LE(peOffset + 4) !== 0x8664
) {
  throw new Error("paste helper 不是 x86_64 Windows PE 文件");
}

const destinationDirectory = resolve(projectRoot, "src-tauri", "binaries");
mkdirSync(destinationDirectory, { recursive: true });
const destination = resolve(
  destinationDirectory,
  `zephyr-paste-helper-${targetTriple}.exe`,
);
copyFileSync(source, destination);
console.log(`paste helper (${profile})：${destination}`);
