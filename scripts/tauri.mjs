import { spawn, spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import {
  closeSync,
  existsSync,
  openSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { loadDeploymentEnvironment } from "./deployment-env.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDirectory, "..");
const args = process.argv.slice(2);
const isDev = args[0] === "dev";
const lockPath = resolve(projectRoot, ".tauri-dev.lock");
const lockToken = randomUUID();
let ownsLock = false;

function processIsAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) {
    return false;
  }

  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}

function readLock() {
  try {
    return JSON.parse(readFileSync(lockPath, "utf8"));
  } catch {
    return null;
  }
}

function removeOwnedLock() {
  if (!ownsLock) {
    return;
  }

  const lock = readLock();
  if (lock?.token === lockToken) {
    try {
      unlinkSync(lockPath);
    } catch (error) {
      if (error?.code !== "ENOENT") {
        console.error(`无法清理开发会话锁：${error.message}`);
      }
    }
  }
  ownsLock = false;
}

function acquireDevLock() {
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      const descriptor = openSync(lockPath, "wx");
      try {
        writeFileSync(
          descriptor,
          JSON.stringify({
            pid: process.pid,
            token: lockToken,
            startedAt: new Date().toISOString(),
          }),
          "utf8",
        );
      } finally {
        closeSync(descriptor);
      }
      ownsLock = true;
      return;
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }

      const existing = readLock();
      if (processIsAlive(existing?.pid)) {
        throw new Error(
          `已有 Zephyr 开发会话正在运行（PID ${existing.pid}）。请先在原终端按 Ctrl+C 停止它。`,
        );
      }

      try {
        unlinkSync(lockPath);
      } catch (unlinkError) {
        if (unlinkError?.code !== "ENOENT") {
          throw unlinkError;
        }
      }
    }
  }

  throw new Error("无法取得 Zephyr 开发会话锁，请删除陈旧的 .tauri-dev.lock 后重试。");
}

if (isDev) {
  try {
    acquireDevLock();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

process.on("exit", removeOwnedLock);

const targetDirectory = resolve(
  process.env.GY_TYPING_CARGO_TARGET_DIR || resolve(projectRoot, "src-tauri", "target"),
);
const deploymentEnvironment = loadDeploymentEnvironment(projectRoot);
const tauriCli = resolve(projectRoot, "node_modules", "@tauri-apps", "cli", "tauri.js");

if (!existsSync(tauriCli)) {
  console.error("未找到本地 Tauri CLI，请先运行 npm install。");
  process.exit(1);
}

let child;
let shuttingDown = false;

function handleShutdown(signal) {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;
  removeOwnedLock();
  process.exitCode = signal === "SIGINT" ? 130 : 143;

  if (child && child.exitCode === null && child.signalCode === null) {
    child.kill(signal);
  }
}

if (args[0] === "dev" || args[0] === "build") {
  const helperBuild = spawnSync(
    process.execPath,
    [
      resolve(scriptDirectory, "build-paste-helper.mjs"),
      ...(args[0] === "build" ? ["--release"] : []),
    ],
    {
      cwd: projectRoot,
      env: {
        ...deploymentEnvironment,
        GY_TYPING_CARGO_TARGET_DIR: targetDirectory,
      },
      stdio: "inherit",
      windowsHide: true,
    },
  );
  if (helperBuild.error || helperBuild.status !== 0) {
    removeOwnedLock();
    console.error(
      `paste helper 构建失败：${helperBuild.error?.message || helperBuild.status}`,
    );
    process.exit(1);
  }
}

process.once("SIGINT", () => handleShutdown("SIGINT"));
process.once("SIGTERM", () => handleShutdown("SIGTERM"));

child = spawn(process.execPath, [tauriCli, ...args], {
  cwd: projectRoot,
  env: {
    ...deploymentEnvironment,
    CARGO_TARGET_DIR: targetDirectory,
  },
  stdio: "inherit",
  windowsHide: false,
});

child.once("error", (error) => {
  removeOwnedLock();
  console.error(`无法启动 Tauri CLI：${error.message}`);
  process.exitCode = 1;
});

child.once("exit", (code, signal) => {
  removeOwnedLock();
  if (shuttingDown) {
    return;
  }
  if (signal) {
    console.error(`Tauri CLI 被信号 ${signal} 终止。`);
    process.exitCode = 1;
    return;
  }
  process.exitCode = code ?? 1;
});

