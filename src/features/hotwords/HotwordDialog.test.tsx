// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/preact";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HotwordDialog } from "./HotwordDialog";

afterEach(cleanup);

describe("HotwordDialog", () => {
  it("keeps organization disabled while loading and exposes service errors", () => {
    render(
      <HotwordDialog
        open
        state={null}
        loading
        notice="endpoint 未授权"
        newWord=""
        edits={{}}
        profileText=""
        appName=""
        appText=""
        appEdits={{}}
        onClose={vi.fn()}
        onRefresh={vi.fn()}
        onOrganize={vi.fn()}
        onNewWord={vi.fn()}
        onAdd={vi.fn()}
        onEdit={vi.fn()}
        onUpdate={vi.fn()}
        onDelete={vi.fn()}
        onProfileText={vi.fn()}
        onSaveProfile={vi.fn()}
        onAppName={vi.fn()}
        onAppText={vi.fn()}
        onSaveApp={vi.fn()}
        onAppDraft={vi.fn()}
        onSaveExistingApp={vi.fn()}
        onDeleteApp={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "整理热词" }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByText("endpoint 未授权")).toBeTruthy();
  });
});
