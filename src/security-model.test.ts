import { describe, expect, it } from "vitest";
import { defaultConfig, type PendingOutput } from "./domain";
import {
  canDeliverPendingOutput,
  configAfterLoadFailure,
  conflictConfig,
  endpointIsTrusted,
  isLatestMutation,
  parseCommandError,
} from "./security-model";

describe("security state model", () => {
  it("keeps the runtime disabled when startup configuration fails", () => {
    expect(configAfterLoadFailure(defaultConfig).enabled).toBe(false);
  });

  it("loads the backend config carried by a revision conflict", () => {
    const currentConfig = { ...defaultConfig, revision: 4 };
    expect(
      conflictConfig({
        code: "config_conflict",
        message: "conflict",
        details: { currentRevision: 4, currentConfig },
      }),
    ).toEqual(currentConfig);
  });

  it("does not treat an authorization rejection as success", () => {
    expect(
      parseCommandError(
        JSON.stringify({ code: "endpoint_authorization_denied", message: "denied" }),
      )?.code,
    ).toBe("endpoint_authorization_denied");
  });

  it("binds endpoint trust to origin and purpose", () => {
    expect(
      endpointIsTrusted(defaultConfig, "https://api.deepseek.com/v1", "hotword_agent"),
    ).toBe(true);
    expect(
      endpointIsTrusted(
        defaultConfig,
        "wss://openspeech.bytedance.com/path",
        "hotword_agent",
      ),
    ).toBe(false);
  });

  it("ignores late mutation responses", () => {
    expect(isLatestMutation(2, 3)).toBe(false);
    expect(isLatestMutation(3, 3)).toBe(true);
  });

  it("disables delivery for expired or unavailable pending output", () => {
    const output: PendingOutput = {
      id: "1",
      sessionId: 1,
      text: "hello",
      executableName: "app.exe",
      createdAtUnixMs: 1,
      expiresAtUnixMs: 100,
      targetAvailable: true,
      reasonCode: "target_changed",
      reasonMessage: "changed",
    };
    expect(canDeliverPendingOutput(output, 99)).toBe(true);
    expect(canDeliverPendingOutput(output, 100)).toBe(false);
    expect(canDeliverPendingOutput({ ...output, targetAvailable: false }, 99)).toBe(false);
  });
});
