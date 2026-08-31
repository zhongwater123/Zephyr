import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const LOCAL_ENV_FILE = ".env.local";

function parseLocalEnvironment(source) {
  const values = {};
  for (const [index, rawLine] of source.split(/\r?\n/).entries()) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) {
      continue;
    }

    const match = line.match(/^([A-Za-z_][A-Za-z0-9_]*)=(.*)$/);
    if (!match) {
      throw new Error(LOCAL_ENV_FILE + " 第 " + (index + 1) + " 行格式无效");
    }

    const [, name, rawValue] = match;
    let value = rawValue.trim();
    if (
      value.length >= 2 &&
      ((value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'")))
    ) {
      value = value.slice(1, -1);
    }
    values[name] = value;
  }
  return values;
}

export function loadDeploymentEnvironment(projectRoot, baseEnvironment = process.env) {
  const environment = { ...baseEnvironment };
  const localEnvPath = resolve(projectRoot, LOCAL_ENV_FILE);
  if (!existsSync(localEnvPath)) {
    return environment;
  }

  const localValues = parseLocalEnvironment(readFileSync(localEnvPath, "utf8"));
  for (const [name, value] of Object.entries(localValues)) {
    environment[name] = value;
  }
  return environment;
}

export function assertBundledCredentials(environment) {
  if (!environment.GY_TYPING_ASR_API_KEY?.trim()) {
    throw new Error(
      "缺少 GY_TYPING_ASR_API_KEY；请在未提交的 .env.local 中提供内部测试 ASR APP Key",
    );
  }
  if (
    environment.GY_TYPING_ASR_AUTH_MODE &&
    environment.GY_TYPING_ASR_AUTH_MODE !== "api_key"
  ) {
    throw new Error("GY_TYPING_ASR_AUTH_MODE 必须为 api_key");
  }
  environment.GY_TYPING_ASR_AUTH_MODE = "api_key";

  if (!environment.GY_TYPING_DEEPSEEK_API_KEY?.trim()) {
    throw new Error(
      "缺少 GY_TYPING_DEEPSEEK_API_KEY；请在未提交的 .env.local 中提供内部测试 DeepSeek API Key",
    );
  }
}
