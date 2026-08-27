// @vitest-environment happy-dom

import { useState } from "preact/hooks";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  defaultConfig,
  type ShortcutEditInterrupted,
  type ShortcutEditOutcome,
  type ShortcutEditSession,
  type ShortcutBinding,
} from "../../domain";

const mocks = vi.hoisted(() => ({
  begin: vi.fn(),
  commit: vi.fn(),
  cancel: vi.fn(),
  trace: vi.fn(),
  getConfig: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("../../ipc/client", () => ({
  configApi: { get: mocks.getConfig },
  shortcutEditApi: {
    begin: mocks.begin,
    commit: mocks.commit,
    cancel: mocks.cancel,
    trace: mocks.trace,
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mocks.listen,
}));

import { ShortcutCaptureField } from "./ShortcutCaptureField";
import { useShortcutBindingController } from "./useShortcutBindingController";

type InterruptionListener = (event: { payload: ShortcutEditInterrupted }) => void;
let interruptionListener: InterruptionListener | null = null;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((next, fail) => {
    resolve = next;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function session(editId = 7): ShortcutEditSession {
  return {
    editId,
    traceId: "server-trace",
    configRevision: defaultConfig.revision,
    activeLabel: defaultConfig.shortcut,
    activeBinding: defaultConfig.shortcut_binding ?? null,
    runtimeState: "suspended",
    message: "快捷键监听已暂停。",
  };
}

function outcome(
  success: boolean,
  activeLabel: string,
  activeBinding: ShortcutBinding | null,
  overrides: Partial<ShortcutEditOutcome> = {},
): ShortcutEditOutcome {
  return {
    success,
    editId: 7,
    traceId: "server-trace",
    configRevision: defaultConfig.revision + (success ? 1 : 0),
    activeLabel,
    activeBinding,
    runtimeState: "active",
    changed: success,
    message: success ? "快捷键已更新。" : "无法注册快捷键。",
    ...overrides,
  };
}

function Harness({
  onNotice = vi.fn(),
  initialConfig = defaultConfig,
}: {
  onNotice?: (message: string) => void;
  initialConfig?: typeof defaultConfig;
}) {
  const [config, setConfig] = useState(initialConfig);
  const controller = useShortcutBindingController(
    config,
    setConfig,
    onNotice,
    (error) => error instanceof Error ? error.message : "快捷键 IPC 失败。",
  );
  return (
    <>
      <ShortcutCaptureField
        view={controller.shortcutView}
        onStart={controller.beginShortcutEdit}
        onCancel={controller.cancelShortcutEdit}
        onKeyDown={controller.handleShortcutKeyDown}
        onKeyUp={controller.handleShortcutKeyUp}
      />
      <span data-testid="saved-shortcut">{config.shortcut}</span>
      {controller.shortcutToast ? <span role="alert">{controller.shortcutToast}</span> : null}
    </>
  );
}

beforeEach(() => {
  mocks.begin.mockReset();
  mocks.commit.mockReset();
  mocks.cancel.mockReset();
  mocks.trace.mockReset();
  mocks.getConfig.mockReset();
  mocks.listen.mockReset();
  interruptionListener = null;
  mocks.listen.mockImplementation(async (_event: string, listener: InterruptionListener) => {
    interruptionListener = listener;
    return vi.fn();
  });
  mocks.trace.mockResolvedValue(undefined);
  mocks.cancel.mockResolvedValue(
    outcome(true, defaultConfig.shortcut, defaultConfig.shortcut_binding ?? null, {
      changed: false,
      configRevision: defaultConfig.revision,
    }),
  );
  mocks.getConfig.mockResolvedValue(defaultConfig);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useShortcutBindingController", () => {
  it("captures immediately while begin is pending, then submits the staged candidate", async () => {
    const begin = deferred<ShortcutEditSession>();
    const nextBinding = {
      modifiers: [
        { kind: "control" as const, side: "left" as const },
        { kind: "shift" as const, side: "right" as const },
      ],
      trigger: { scanCode: 0x25, extended: false },
    };
    mocks.begin.mockReturnValue(begin.promise);
    mocks.commit.mockResolvedValue(outcome(true, "左 Ctrl+右 Shift+K", nextBinding));

    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));
    let field = screen.getByRole("button", { name: /正在录入快捷键/ });

    fireEvent.keyDown(field, {
      code: "ControlLeft",
      key: "Control",
      location: 1,
      ctrlKey: true,
    });
    expect(screen.getByText("左 Ctrl")).toBeTruthy();

    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "ShiftRight",
      key: "Shift",
      location: 2,
      ctrlKey: true,
      shiftKey: true,
    });
    expect(screen.getByText("右 Shift")).toBeTruthy();

    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "KeyK",
      key: "k",
      ctrlKey: true,
      shiftKey: true,
    });
    expect(screen.getByText("K")).toBeTruthy();
    expect(screen.getByRole("button", { name: /正在应用快捷键/ })).toHaveProperty(
      "disabled",
      true,
    );
    expect(mocks.commit).not.toHaveBeenCalled();

    await act(async () => {
      begin.resolve(session());
      await begin.promise;
    });

    await waitFor(() => expect(mocks.commit).toHaveBeenCalledOnce());
    expect(mocks.commit).toHaveBeenCalledWith(
      expect.any(String),
      7,
      defaultConfig.revision,
      nextBinding,
    );
    await waitFor(() =>
      expect(screen.getByTestId("saved-shortcut").textContent).toBe("左 Ctrl+右 Shift+K"),
    );

    const rawKeyTrace = mocks.trace.mock.calls
      .map(([input]) => input)
      .find((input) => input.event === "dom_keydown" && input.code === "KeyK");
    expect(rawKeyTrace).toMatchObject({
      key: "k",
      repeat: false,
      ctrl: true,
      shift: true,
    });
    expect(rawKeyTrace.eventSeq).toBeGreaterThan(0);
  });

  it("keeps an invalid candidate in capture and accepts the next combination", async () => {
    const nextBinding = {
      modifiers: [{ kind: "control" as const, side: "left" as const }],
      trigger: { scanCode: 0x25, extended: false },
    };
    mocks.begin.mockResolvedValue(session());
    mocks.commit.mockResolvedValue(outcome(true, "左 Ctrl+K", nextBinding));

    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));
    let field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, { code: "KeyK", key: "k" });

    expect(screen.getByRole("alert").textContent).toContain("需要与修饰键组合");
    expect(mocks.commit).not.toHaveBeenCalled();

    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "ControlLeft",
      key: "Control",
      location: 1,
      ctrlKey: true,
    });
    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, { code: "KeyK", key: "k", ctrlKey: true });

    await waitFor(() => expect(mocks.commit).toHaveBeenCalledOnce());
  });

  it("rolls the optimistic label back when runtime registration fails", async () => {
    const onNotice = vi.fn();
    mocks.begin.mockResolvedValue(session());
    mocks.commit.mockResolvedValue(
      outcome(false, defaultConfig.shortcut, defaultConfig.shortcut_binding ?? null, {
        errorCode: "hook_unavailable",
        message: "无法注册快捷键。",
      }),
    );

    render(<Harness onNotice={onNotice} />);
    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));
    let field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "ControlRight",
      key: "Control",
      location: 2,
      ctrlKey: true,
    });
    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, { code: "KeyK", key: "k", ctrlKey: true });

    await waitFor(() => {
      expect(
        screen.getAllByRole("alert").some((alert) =>
          alert.textContent?.includes("无法注册快捷键"),
        ),
      ).toBe(true);
    });
    expect(screen.getByTestId("saved-shortcut").textContent).toBe(defaultConfig.shortcut);
    expect(screen.getByText("Space")).toBeTruthy();
    expect(onNotice).toHaveBeenCalledWith("无法注册快捷键。");
  });

  it("shows that a disabled shortcut was saved for the next enable", async () => {
    const disabledConfig = { ...defaultConfig, enabled: false };
    const nextBinding = {
      modifiers: [{ kind: "control" as const, side: "left" as const }],
      trigger: { scanCode: 0x25, extended: false },
    };
    mocks.begin.mockResolvedValue({ ...session(), runtimeState: "disabled" });
    mocks.commit.mockResolvedValue(
      outcome(true, "左 Ctrl+K", nextBinding, {
        runtimeState: "disabled",
        message: "快捷键已保存，开启后生效。",
      }),
    );

    render(<Harness initialConfig={disabledConfig} />);
    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));
    let field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "ControlLeft",
      key: "Control",
      location: 1,
      ctrlKey: true,
    });
    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, { code: "KeyK", key: "k", ctrlKey: true });

    await waitFor(() =>
      expect(screen.getByText("快捷键已保存，开启后生效。")).toBeTruthy(),
    );
    expect(screen.getByTestId("saved-shortcut").textContent).toBe("左 Ctrl+K");
  });

  it("cancels a backend edit on bare Escape", async () => {
    mocks.begin.mockResolvedValue(session());
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));
    await waitFor(() => expect(mocks.begin).toHaveBeenCalledOnce());

    const field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, { code: "Escape", key: "Escape" });

    await waitFor(() =>
      expect(mocks.cancel).toHaveBeenCalledWith(expect.any(String), 7),
    );
    expect(screen.getByTestId("saved-shortcut").textContent).toBe(defaultConfig.shortcut);
  });

  it("rejects an old interruption and exits capture for the current edit", async () => {
    mocks.begin.mockImplementation(async (traceId: string) => ({
      ...session(),
      traceId,
    }));
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));
    await waitFor(() => expect(mocks.begin).toHaveBeenCalledOnce());
    await waitFor(() => expect(interruptionListener).not.toBeNull());
    const traceId = mocks.begin.mock.calls[0][0] as string;

    act(() => {
      interruptionListener?.({
        payload: {
          outcome: outcome(
            false,
            defaultConfig.shortcut,
            defaultConfig.shortcut_binding ?? null,
            {
              traceId: "old-trace",
              errorCode: "hook_interrupted",
              message: "旧事件",
            },
          ),
        },
      });
    });
    expect(screen.getByRole("button", { name: /正在录入快捷键/ })).toBeTruthy();

    act(() => {
      interruptionListener?.({
        payload: {
          outcome: outcome(
            false,
            defaultConfig.shortcut,
            defaultConfig.shortcut_binding ?? null,
            {
              traceId,
              errorCode: "hook_interrupted",
              runtimeState: "error",
              message: "键盘 Hook 已中断。",
            },
          ),
        },
      });
    });
    await waitFor(() => {
      expect(screen.getAllByRole("alert").some((item) =>
        item.textContent?.includes("键盘 Hook 已中断"),
      )).toBe(true);
    });
    expect(screen.queryByRole("button", { name: /正在录入快捷键/ })).toBeNull();
  });

  it("removes AltGr synthetic left Control from the candidate", () => {
    const begin = deferred<ShortcutEditSession>();
    mocks.begin.mockReturnValue(begin.promise);
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));

    let field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "ControlLeft",
      key: "Control",
      location: 1,
      ctrlKey: true,
    });
    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "AltRight",
      key: "AltGraph",
      location: 2,
      ctrlKey: true,
      altKey: true,
    });
    expect(screen.queryByText("左 Ctrl")).toBeNull();
    expect(screen.getByText("右 Alt")).toBeTruthy();

    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "KeyK",
      key: "k",
      ctrlKey: true,
      altKey: true,
    });
    expect(screen.queryByText("左 Ctrl")).toBeNull();
    expect(screen.getByText("右 Alt")).toBeTruthy();
    expect(screen.getByText("K")).toBeTruthy();
  });

  it("finalizes a supported modifier-only binding after 200ms", () => {
    const begin = deferred<ShortcutEditSession>();
    mocks.begin.mockReturnValue(begin.promise);
    const now = vi.spyOn(performance, "now").mockReturnValue(0);
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /点击更改/ }));

    let field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyDown(field, {
      code: "ControlRight",
      key: "Control",
      location: 2,
      ctrlKey: true,
    });
    now.mockReturnValue(200);
    field = screen.getByRole("button", { name: /正在录入快捷键/ });
    fireEvent.keyUp(field, {
      code: "ControlRight",
      key: "Control",
      location: 2,
    });

    expect(screen.getByRole("button", { name: /正在应用快捷键/ })).toHaveProperty(
      "disabled",
      true,
    );
    expect(screen.getByText("右 Ctrl")).toBeTruthy();
    now.mockRestore();
  });
});
