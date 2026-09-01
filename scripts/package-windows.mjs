import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  readFileSync,
  readdirSync,
  mkdirSync,
  renameSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  assertBundledCredentials,
  loadDeploymentEnvironment,
} from "./deployment-env.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDirectory, "..");
const checkOnly = process.argv.slice(2).includes("--check-only");
const distributionName = "Zephyr";
const deploymentEnvironment = loadDeploymentEnvironment(projectRoot);
const targetRoot = resolve(
  deploymentEnvironment.GY_TYPING_CARGO_TARGET_DIR ||
    resolve(projectRoot, "src-tauri", "target"),
);
deploymentEnvironment.CARGO_TARGET_DIR = targetRoot;
const packagingTempDirectory = resolve(targetRoot, ".tauri-packaging-tmp");
mkdirSync(packagingTempDirectory, { recursive: true });
deploymentEnvironment.TEMP = packagingTempDirectory;
deploymentEnvironment.TMP = packagingTempDirectory;

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function run(command, args, label) {
  console.log(`\n==> ${label}`);
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    env: deploymentEnvironment,
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

function pasteProtocolVersion() {
  const source = readFileSync(
    resolve(projectRoot, "src-tauri", "crates", "paste-protocol", "src", "lib.rs"),
    "utf8",
  );
  const version = source.match(/pub const PROTOCOL_VERSION:\s*u16\s*=\s*(\d+)\s*;/)?.[1];
  if (!version) {
    throw new Error("无法读取 paste helper 协议版本");
  }
  return Number(version);
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
  if (tauriConfig.bundle?.useLocalToolsDir !== true) {
    throw new Error("Windows 打包必须启用 bundle.useLocalToolsDir，避免 NSIS 工具缓存跨磁盘移动失败");
  }
  if (tauriConfig.bundle?.windows?.nsis?.installMode !== "currentUser") {
    throw new Error("测试安装包必须使用 NSIS currentUser 安装模式");
  }
  if (
    !Array.isArray(tauriConfig.bundle?.externalBin) ||
    tauriConfig.bundle.externalBin.length !== 1 ||
    tauriConfig.bundle.externalBin[0] !== "binaries/zephyr-paste-helper"
  ) {
    throw new Error("Tauri externalBin 必须且只能注册 zephyr-paste-helper sidecar");
  }

  return {
    mainBinaryName: packageJson.name,
    productName: tauriConfig.productName,
    identifier: tauriConfig.identifier,
    version: tauriConfig.version,
    helperPath: resolve(
      projectRoot,
      "src-tauri",
      "binaries",
      "zephyr-paste-helper-x86_64-pc-windows-msvc.exe",
    ),
    helperProtocolVersion: pasteProtocolVersion(),
  };
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function findInstaller(bundleDirectory, productName, version) {
  if (!existsSync(bundleDirectory)) {
    throw new Error(`未找到 NSIS 输出目录：${bundleDirectory}`);
  }

  const expectedName = `${productName}_${version}_x64-setup.exe`;
  const expectedPath = resolve(bundleDirectory, expectedName);
  if (existsSync(expectedPath)) {
    return expectedPath;
  }

  const candidates = readdirSync(bundleDirectory)
    .filter(
      (name) =>
        name.endsWith("-setup.exe") &&
        name.includes(version) &&
        !name.startsWith(`${distributionName}_`),
    )
    .map((name) => resolve(bundleDirectory, name));
  if (candidates.length !== 1) {
    throw new Error(
      `预期找到一个 ${version} NSIS 安装包，实际找到 ${candidates.length} 个`,
    );
  }
  return candidates[0];
}

function renameInstaller(installerPath, version) {
  const destination = resolve(
    dirname(installerPath),
    `${distributionName}_${version}_x64-setup.exe`,
  );
  if (installerPath === destination) {
    return destination;
  }
  if (existsSync(destination)) {
    unlinkSync(destination);
  }
  renameSync(installerPath, destination);
  return destination;
}

function assertWindowsGuiSubsystem(binaryPath) {
  const binary = readFileSync(binaryPath);
  if (binary.length < 0x40 || binary.toString("ascii", 0, 2) !== "MZ") {
    throw new Error(`release 主程序不是有效的 Windows PE 文件：${binaryPath}`);
  }

  const peOffset = binary.readUInt32LE(0x3c);
  const optionalHeaderOffset = peOffset + 24;
  if (
    peOffset + 4 > binary.length ||
    binary.toString("binary", peOffset, peOffset + 4) !== "PE\0\0" ||
    optionalHeaderOffset + 70 > binary.length
  ) {
    throw new Error(`release 主程序的 PE 头不完整：${binaryPath}`);
  }

  const optionalHeaderMagic = binary.readUInt16LE(optionalHeaderOffset);
  if (optionalHeaderMagic !== 0x10b && optionalHeaderMagic !== 0x20b) {
    throw new Error(`release 主程序使用未知的 PE Optional Header：${binaryPath}`);
  }

  const subsystem = binary.readUInt16LE(optionalHeaderOffset + 68);
  if (subsystem !== 2) {
    throw new Error(
      `release 主程序必须使用 Windows GUI 子系统，当前 PE subsystem=${subsystem}`,
    );
  }
}

function assertHelper(contract) {
  if (!existsSync(contract.helperPath)) {
    throw new Error(`未找到 paste helper sidecar：${contract.helperPath}`);
  }
  const binary = readFileSync(contract.helperPath);
  if (binary.length < 0x40 || binary.toString("ascii", 0, 2) !== "MZ") {
    throw new Error("paste helper 不是有效的 Windows PE 文件");
  }
  const peOffset = binary.readUInt32LE(0x3c);
  if (
    peOffset + 6 > binary.length ||
    binary.toString("binary", peOffset, peOffset + 4) !== "PE\0\0" ||
    binary.readUInt16LE(peOffset + 4) !== 0x8664
  ) {
    throw new Error("paste helper PE 架构不是 AMD64");
  }
  const transactionId = "00000000-0000-0000-0000-000000000001";
  const selfCheck = spawnSync(contract.helperPath, [], {
    cwd: projectRoot,
    input: `${JSON.stringify({
      protocolVersion: contract.helperProtocolVersion,
      operation: "selfCheck",
      transactionId,
      mode: null,
      text: null,
      target: null,
    })}\n`,
    encoding: "utf8",
    windowsHide: true,
  });
  if (selfCheck.error || selfCheck.status !== 0) {
    throw new Error(
      `paste helper 自检失败：${selfCheck.error?.message || selfCheck.stderr || selfCheck.status}`,
    );
  }
  const event = JSON.parse(selfCheck.stdout.trim().split(/\r?\n/)[0] || "null");
  if (
    event?.protocolVersion !== contract.helperProtocolVersion ||
    event?.transactionId !== transactionId ||
    event?.kind !== "selfCheck" ||
    typeof event?.helperVersion !== "string"
  ) {
    throw new Error("paste helper 自检回执与共享协议不匹配");
  }
  const rejectedFault = spawnSync(contract.helperPath, [], {
    cwd: projectRoot,
    input: `${JSON.stringify({
      protocolVersion: contract.helperProtocolVersion,
      operation: "selfCheck",
      transactionId,
      mode: null,
      text: null,
      target: null,
      sendInputCount: 0,
    })}\n`,
    encoding: "utf8",
    windowsHide: true,
  });
  const rejectedEvent = JSON.parse(
    rejectedFault.stdout.trim().split(/\r?\n/)[0] || "null",
  );
  if (
    rejectedFault.status === 0 ||
    rejectedEvent?.code !== "fault_injection_disabled" ||
    rejectedEvent?.receipt?.submission !== "notSubmitted"
  ) {
    throw new Error("release paste helper 没有拒绝故障注入字段");
  }
  return {
    fileName: contract.helperPath.split(/[\\/]/).at(-1),
    bytes: statSync(contract.helperPath).size,
    sha256: sha256(contract.helperPath),
    protocolVersion: contract.helperProtocolVersion,
    helperVersion: event.helperVersion,
    peMachine: "AMD64",
  };
}

function assertRuntimeHelper(targetRoot, sourceHelper) {
  const runtimePath = resolve(
    targetRoot,
    "release",
    "zephyr-paste-helper.exe",
  );
  if (!existsSync(runtimePath)) {
    throw new Error(`Tauri release 目录缺少 paste helper：${runtimePath}`);
  }
  if (sha256(runtimePath) !== sourceHelper.sha256) {
    throw new Error("Tauri release 目录中的 paste helper 与已校验 sidecar 不一致");
  }
  return runtimePath;
}

function buildManifest(contract, installerPath, helper) {
  const gitRevision = capture("git", ["rev-parse", "HEAD"]);
  const gitStatus = capture("git", ["status", "--porcelain"]);
  const manifest = {
    schemaVersion: 2,
    productName: contract.productName,
    distributionName,
    identifier: contract.identifier,
    version: contract.version,
    target: "windows-x86_64",
    bundleType: "nsis",
    windowsSubsystem: "windows-gui",
    installer: {
      fileName: installerPath.split(/[\\/]/).at(-1),
      bytes: statSync(installerPath).size,
      sha256: sha256(installerPath),
    },
    pasteHelper: helper,
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
  assertBundledCredentials(deploymentEnvironment);
  console.log(
    `发布契约检查通过：${contract.productName} ${contract.version} (${contract.identifier})`,
  );

  run(
    process.execPath,
    ["scripts/build-paste-helper.mjs", "--release"],
    "构建并校验 release paste helper",
  );
  const helper = assertHelper(contract);

  run(process.execPath, ["scripts/check-architecture-docs.mjs"], "架构文档结构检查");
  run(process.execPath, ["scripts/check-asr-boundaries.mjs"], "ASR 边界检查");
  run(
    process.execPath,
    ["scripts/check-platform-boundaries.mjs"],
    "共享平台边界检查",
  );
  run(process.execPath, ["scripts/check-secrets.mjs"], "凭据扫描");
  run(
    process.execPath,
    ["node_modules/vitest/vitest.mjs", "run"],
    "前端自动化测试",
  );
  run(
    "cargo",
    [
      "test",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--workspace",
      "--all-features",
    ],
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

  const runtimeHelperPath = assertRuntimeHelper(targetRoot, helper);
  console.log(`Tauri runtime helper：${runtimeHelperPath}`);

  assertWindowsGuiSubsystem(
    resolve(targetRoot, "release", `${contract.mainBinaryName}.exe`),
  );
  const bundleDirectory = resolve(targetRoot, "release", "bundle", "nsis");
  const installerPath = renameInstaller(
    findInstaller(bundleDirectory, contract.productName, contract.version),
    contract.version,
  );
  const { manifest, manifestPath } = buildManifest(contract, installerPath, helper);
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
