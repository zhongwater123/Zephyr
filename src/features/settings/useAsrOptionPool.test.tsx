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

function Harness() {
  const controller = useAsrOptionPool(vi.fn());
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
});
