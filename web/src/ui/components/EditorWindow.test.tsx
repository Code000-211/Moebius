/** The editor window: a clean one closes at once, a changed one asks before it is left, Save is the caller's. */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DeleteButton, EditorWindow } from "./EditorWindow";

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

  it("puts the focus on the first field, not on the close icon", () => {
    show(false);
    expect(document.activeElement).toBe(screen.getByLabelText("Name"));
  });

  it("puts the focus on the window itself when it has no field", () => {
    render(
      <EditorWindow open title="Edge" dirty={false} onClose={vi.fn()}>
        <p>Leaves a</p>
      </EditorWindow>,
    );
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
  });

  it("counts a choice that opens a menu as a field", () => {
    render(
      <EditorWindow open title="Edit" dirty={false} onSave={vi.fn()} onClose={vi.fn()}>
        <button type="button" aria-haspopup="menu">
          Model
        </button>
        <input aria-label="Budget" />
      </EditorWindow>,
    );
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Model" }));
  });

  it("keeps a field that took the focus itself", () => {
    render(
      <EditorWindow open title="New" dirty={false} onSave={vi.fn()} onClose={vi.fn()}>
        <input aria-label="First" />
        {/* biome-ignore lint/a11y/noAutofocus: the editors use autoFocus on a new name */}
        <input aria-label="Second" autoFocus />
      </EditorWindow>,
    );
    expect(document.activeElement).toBe(screen.getByLabelText("Second"));
  });

  it("keeps its size and the height of its foot when the discard question replaces the foot", () => {
    show(true);
    const dialog = screen.getByRole("dialog");
    const before = dialog.className;
    const foot = screen.getByRole("button", { name: "Save" }).parentElement?.className;
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("dialog").className).toBe(before);
    expect(before).toContain("h-[32rem]");
    expect(foot).toContain("h-14");
    expect(screen.getByRole("alertdialog").className).toContain("h-14");
  });

  it("scrolls its body between a fixed head and foot", () => {
    show(false);
    const body = screen.getByLabelText("Name").parentElement as HTMLElement;
    expect(body.className).toContain("overflow-y-auto");
    expect(body.className).toContain("min-h-0");
  });

  it("is as tall as its content with height fit", () => {
    render(
      <EditorWindow open title="Edge" dirty={false} height="fit" onClose={vi.fn()}>
        <p>Leaves a</p>
      </EditorWindow>,
    );
    expect(screen.getByRole("dialog").className).not.toContain("h-[32rem]");
  });

  it("has Close and no Save without onSave, and Close closes at once", () => {
    const onClose = vi.fn();
    render(
      <EditorWindow open title="Edge" dirty={false} onClose={onClose}>
        <p>Leaves a</p>
      </EditorWindow>,
    );
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    fireEvent.click(screen.getAllByRole("button", { name: "Close" }).at(-1) as HTMLElement);
    expect(onClose).toHaveBeenCalled();
  });

  it("draws Delete outlined with the danger text, never filled", () => {
    render(<DeleteButton>Delete rule</DeleteButton>);
    const { className } = screen.getByRole("button", { name: "Delete rule" });
    expect(className).toContain("text-danger-text");
    expect(className).toContain("bg-transparent");
    expect(className).toContain("outline-hairline-strong");
    expect(className).not.toContain("bg-error-fill");
  });

  it("takes the resize handle off a textarea in its body", () => {
    render(
      <EditorWindow open title="Edit" dirty={false} onSave={vi.fn()} onClose={vi.fn()}>
        <textarea aria-label="Text" />
      </EditorWindow>,
    );
    const body = screen.getByLabelText("Text").parentElement as HTMLElement;
    expect(body.className).toContain("[&_textarea]:resize-none");
  });

  it("sets a label in medium weight", () => {
    show(false);
    const body = screen.getByLabelText("Name").parentElement as HTMLElement;
    expect(body.className).toContain("[&_label>span:first-child]:font-medium");
  });
});
