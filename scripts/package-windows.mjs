import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDirectory, "..");
const checkOnly = process.argv.slice(2).includes("--check-only");

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function run(command, args, label) {
  console.log(`\n==> ${label}`);
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    env: process.env,
    stdio: "inherit",
    windowsHide: true,
  });

  if (result.error) {
    throw new Error(`${label} 无法启动：${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`${label} 失败（退出码 ${result.status ?? "unknown"}）`);
  }
}

function capture(command, args) {
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    encoding: "utf8",
    windowsHide: true,
  });
  return result.status === 0 ? result.stdout.trim() : null;
}

function cargoVersion() {
  const cargoToml = readFileSync(resolve(projectRoot, "src-tauri", "Cargo.toml"), "utf8");
  const packageSection = cargoToml.match(/\[package\]([\s\S]*?)(?:\r?\n\[|$)/)?.[1];
  const version = packageSection?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) {
    throw new Error("无法从 src-tauri/Cargo.toml 的 [package] 读取版本号");
  }
  return version;
}

function assertReleaseContract() {
  if (process.platform !== "win32") {
    throw new Error("Windows NSIS 安装包必须在 Windows 上构建");
  }

  const packageJson = readJson(resolve(projectRoot, "package.json"));
  const tauriConfig = readJson(resolve(projectRoot, "src-tauri", "tauri.conf.json"));
  const versions = {
    "package.json": packageJson.version,
    "src-tauri/tauri.conf.json": tauriConfig.version,
    "src-tauri/Cargo.toml": cargoVersion(),
  };
  const uniqueVersions = new Set(Object.values(versions));
  if (uniqueVersions.size !== 1) {
    throw new Error(
      `发布版本不一致：${Object.entries(versions)
        .map(([file, version]) => `${file}=${version}`)
        .join(", ")}`,
    );
  }

  const targets = tauriConfig.bundle?.targets;
  if (!Array.isArray(targets) || !targets.includes("nsis")) {
    throw new Error("src-tauri/tauri.conf.json 必须启用 NSIS bundle target");
  }
  if (tauriConfig.bundle?.windows?.nsis?.installMode !== "currentUser") {
    throw new Error("测试安装包必须使用 NSIS currentUser 安装模式");
  }

  return {
    productName: tauriConfig.productName,
    identifier: tauriConfig.identifier,
    version: tauriConfig.version,
  };
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function findInstaller(bundleDirectory, version) {
  if (!existsSync(bundleDirectory)) {
    throw new Error(`未找到 NSIS 输出目录：${bundleDirectory}`);
  }

  const candidates = readdirSync(bundleDirectory)
    .filter((name) => name.endsWith("-setup.exe") && name.includes(version))
    .map((name) => resolve(bundleDirectory, name));
  if (candidates.length !== 1) {
    throw new Error(
      `预期找到一个 ${version} NSIS 安装包，实际找到 ${candidates.length} 个`,
    );
  }
  return candidates[0];
}

function buildManifest(contract, installerPath) {
  const gitRevision = capture("git", ["rev-parse", "HEAD"]);
  const gitStatus = capture("git", ["status", "--porcelain"]);
  const manifest = {
    schemaVersion: 1,
    productName: contract.productName,
    identifier: contract.identifier,
    version: contract.version,
    target: "windows-x86_64",
    bundleType: "nsis",
    installer: {
      fileName: installerPath.split(/[\\/]/).at(-1),
      bytes: statSync(installerPath).size,
      sha256: sha256(installerPath),
    },
    source: {
      gitRevision,
      dirty: Boolean(gitStatus),
    },
    builtAt: new Date().toISOString(),
  };
  const manifestPath = resolve(dirname(installerPath), "release-manifest.json");
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  return { manifest, manifestPath };
}

try {
  const contract = assertReleaseContract();
  console.log(
    `发布契约检查通过：${contract.productName} ${contract.version} (${contract.identifier})`,
  );

  run(process.execPath, ["scripts/check-architecture-docs.mjs"], "架构文档结构检查");
  run(process.execPath, ["scripts/check-asr-boundaries.mjs"], "ASR 边界检查");
  run(process.execPath, ["scripts/check-secrets.mjs"], "凭据扫描");
  run(
    process.execPath,
    ["node_modules/vitest/vitest.mjs", "run"],
    "前端自动化测试",
  );
  run(
    "cargo",
    ["test", "--manifest-path", "src-tauri/Cargo.toml", "--all-features"],
    "Rust 自动化测试",
  );

  if (checkOnly) {
    console.log("\nWindows 打包前检查通过；--check-only 未生成安装包。");
    process.exit(0);
  }

  run(
    process.execPath,
    ["scripts/tauri.mjs", "build", "--bundles", "nsis"],
    "Tauri NSIS 构建",
  );

  const targetRoot = resolve(
    process.env.GY_TYPING_CARGO_TARGET_DIR || resolve(projectRoot, "src-tauri", "target"),
  );
  const installerPath = findInstaller(
    resolve(targetRoot, "release", "bundle", "nsis"),
    contract.version,
  );
  const { manifest, manifestPath } = buildManifest(contract, installerPath);
  console.log(`\n安装包：${installerPath}`);
  console.log(`发布清单：${manifestPath}`);
  console.log(`SHA-256：${manifest.installer.sha256}`);
  if (manifest.source.dirty) {
    console.warn("注意：该安装包来自脏工作树，只适合可追踪的内部测试，不应作为正式发布版本。");
  }
} catch (error) {
  console.error(`\nWindows 打包失败：${error.message}`);
  process.exit(1);
}
