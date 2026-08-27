// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/preact";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AsrOptionPool } from "../../domain";

const initialPool: AsrOptionPool = {
  providerId: "volcengine",
  providerDisplayName: "火山引擎",
  schemaVersion: 1,
  revision: 1,
  options: [],
  values: {},
};
const currentPool: AsrOptionPool = { ...initialPool, revision: 2 };

const mocks = vi.hoisted(() => ({
  getOptionPool: vi.fn(),
  setOption: vi.fn(),
}));

vi.mock("../../ipc/client", () => ({
  asrApi: {
    getOptionPool: mocks.getOptionPool,
    setOption: mocks.setOption,
  },
}));

import { useAsrOptionPool } from "./useAsrOptionPool";

beforeEach(() => {
  mocks.getOptionPool.mockResolvedValue(initialPool);
  mocks.setOption.mockRejectedValue({
    code: "config_conflict",
    message: "conflict",
    details: { currentRevision: 2, currentPool },
  });
});

afterEach(() => {
  cleanup();
  mocks.getOptionPool.mockReset();
  mocks.setOption.mockReset();
});

const noopNotice = () => {};

function Harness({ onNotice = noopNotice }: { onNotice?: (message: string) => void }) {
  const controller = useAsrOptionPool(onNotice);
  return (
    <div>
      <button onClick={() => void controller.load()}>load</button>
      <button
        onClick={() =>
          void controller.setOption("punctuation", { type: "boolean", value: false })
        }
      >
        save
      </button>
      <span data-testid="revision">{controller.pool?.revision ?? 0}</span>
      <span data-testid="saving">{String(controller.savingOptions.punctuation ?? false)}</span>
      <span data-testid="error">{controller.errors.punctuation ?? ""}</span>
    </div>
  );
}

describe("useAsrOptionPool", () => {
  it("reloads the pool carried by a revision conflict", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "load" }));
    await waitFor(() => expect(screen.getByTestId("revision").textContent).toBe("1"));
    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() => expect(screen.getByTestId("revision").textContent).toBe("2"));
    expect(mocks.setOption).toHaveBeenCalledTimes(1);
  });

  it("persists an option with the loaded revision and accepts the returned pool", async () => {
    mocks.setOption.mockResolvedValue(currentPool);
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "load" }));
    await waitFor(() => expect(screen.getByTestId("revision").textContent).toBe("1"));

    fireEvent.click(screen.getByRole("button", { name: "save" }));

    await waitFor(() => expect(screen.getByTestId("revision").textContent).toBe("2"));
    expect(mocks.setOption).toHaveBeenCalledWith({
      optionId: "punctuation",
      value: { type: "boolean", value: false },
      expectedRevision: 1,
    });
    expect(screen.getByTestId("error").textContent).toBe("");
  });

  it("reports a load failure without replacing the current pool", async () => {
    const onNotice = vi.fn();
    mocks.getOptionPool.mockRejectedValue({ code: "network", message: "offline" });
    render(<Harness onNotice={onNotice} />);

    fireEvent.click(screen.getByRole("button", { name: "load" }));

    await waitFor(() =>
      expect(onNotice).toHaveBeenCalledWith("识别选项加载失败：offline"),
    );
    expect(screen.getByTestId("revision").textContent).toBe("0");
  });

  it("restores and reloads after a non-conflict save failure", async () => {
    mocks.getOptionPool
      .mockResolvedValueOnce(initialPool)
      .mockResolvedValueOnce(currentPool);
    mocks.setOption.mockRejectedValue({ code: "config_write_failed", message: "disk full" });
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "load" }));
    await waitFor(() => expect(screen.getByTestId("revision").textContent).toBe("1"));

    fireEvent.click(screen.getByRole("button", { name: "save" }));

    await waitFor(() => expect(screen.getByTestId("revision").textContent).toBe("2"));
    expect(screen.getByTestId("error").textContent).toBe("disk full");
    expect(mocks.getOptionPool).toHaveBeenCalledTimes(2);
  });

  it("ignores saves before load and duplicate saves while one is in flight", async () => {
    let resolveSave!: (pool: AsrOptionPool) => void;
    mocks.setOption.mockImplementation(
      () => new Promise<AsrOptionPool>((resolve) => (resolveSave = resolve)),
    );
    render(<Harness />);

    fireEvent.click(screen.getByRole("button", { name: "save" }));
    expect(mocks.setOption).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "load" }));
    await waitFor(() => expect(screen.getByTestId("revision").textContent).toBe("1"));
    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() => expect(screen.getByTestId("saving").textContent).toBe("true"));
    fireEvent.click(screen.getByRole("button", { name: "save" }));
    expect(mocks.setOption).toHaveBeenCalledTimes(1);

    resolveSave(currentPool);
    await waitFor(() => expect(screen.getByTestId("saving").textContent).toBe("false"));
    expect(screen.getByTestId("revision").textContent).toBe("2");
  });
});
