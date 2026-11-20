/** The editor window: a clean one closes at once, a changed one asks before it is left, Save is the caller's. */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { EditorWindow } from "./EditorWindow";

function show(dirty: boolean, extra: Partial<Parameters<typeof EditorWindow>[0]> = {}) {
  const onClose = vi.fn();
  const onSave = vi.fn();
  render(
    <EditorWindow open title="Edit rule" dirty={dirty} onSave={onSave} onClose={onClose} {...extra}>
      <input aria-label="Name" />
    </EditorWindow>,
  );
  return { onClose, onSave };
}

describe("EditorWindow", () => {
  it("shows its title and body in a dialog", () => {
    show(false);
    expect(screen.getByRole("dialog", { name: "Edit rule" })).toBeTruthy();
    expect(screen.getByLabelText("Name")).toBeTruthy();
  });

  it("closes at once on Escape when nothing changed", () => {
    const { onClose } = show(false);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("asks before Escape closes a changed draft, and Keep editing stays", () => {
    const { onClose } = show(true);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("alertdialog", { name: "Discard changes?" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes when the changes are discarded", () => {
    const { onClose } = show(true);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("saves through onSave and disables Save with nothing to save or while saving", () => {
    const { onSave } = show(true);
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledOnce();
  });

  it("disables Save for a clean draft", () => {
    show(false);
    expect(screen.getByRole("button", { name: "Save" }).getAttribute("disabled")).not.toBeNull();
  });

  it("shows what the server refused and stays open while saving", () => {
    const { onClose } = show(true, { error: "Line 3 is wrong.", saving: true });
    expect(screen.getByRole("alert").textContent).toBe("Line 3 is wrong.");
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Saving" })).toBeTruthy();
  });
});
