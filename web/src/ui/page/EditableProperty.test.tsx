// @vitest-environment jsdom
/**
 * A property edited in place: each kind of value opens its own editor from the keyboard and saves alone, Escape leaves
 * without saving and gives the focus back, and a refusal is said under the property and read as the value's
 * description, the value staying what it was.
 */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { EditableProperty, ListProperty, type PropertyEditor } from "./EditableProperty";

afterEach(cleanup);

/** The text of what an element's `aria-describedby` points at, as a screen reader reads it after its name. */
function description(element: HTMLElement): string {
  return (element.getAttribute("aria-describedby") ?? "")
    .split(" ")
    .filter((id) => id !== "")
    .map((id) => document.getElementById(id)?.textContent ?? "")
    .join(" ");
}

/** A save that resolves at once, recording what it was given. */
const saving = () => vi.fn(async (_value: unknown) => {});

function property(editor: PropertyEditor, shown: string) {
  return render(
    <EditableProperty name="Field" editor={editor}>
      {shown}
    </EditableProperty>,
  );
}

describe("a value edited in place", () => {
  it("is a button named by the property and its value", () => {
    property({ kind: "text", value: "Fix it", onSave: saving() }, "Fix it");
    expect(screen.getByRole("button", { name: "Field Fix it" })).toBeTruthy();
  });

  it("chooses from a menu with the keyboard and saves the choice alone", async () => {
    const onSave = saving();
    property(
      {
        kind: "choice",
        options: [
          ["open", "Open"],
          ["closed", "Closed"],
        ],
        value: "open",
        onSave,
      },
      "Open",
    );
    const value = screen.getByRole("button", { name: "Field Open" });
    expect(value.getAttribute("aria-haspopup")).toBe("menu");
    fireEvent.keyDown(value, { key: "Enter" });
    const closed = await screen.findByRole("menuitemradio", { name: "Closed" });
    expect(screen.getByRole("menuitemradio", { name: "Open" }).getAttribute("aria-checked")).toBe(
      "true",
    );
    fireEvent.click(closed);
    await waitFor(() => expect(onSave).toHaveBeenCalledWith("closed"));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("chooses a person, or nobody", async () => {
    const onSave = saving();
    property(
      { kind: "person", people: [["person", "Me"]], none: "No one", value: "person", onSave },
      "person",
    );
    fireEvent.keyDown(screen.getByRole("button", { name: "Field person" }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitemradio", { name: "No one" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(null));
  });

  it("saves typed text on Enter, and the focus comes back to the value", async () => {
    const onSave = saving();
    property({ kind: "text", value: "Fix it", onSave }, "Fix it");
    const value = screen.getByRole("button", { name: "Field Fix it" });
    fireEvent.click(value);
    const field = screen.getByRole("textbox", { name: "Field" });
    expect(document.activeElement).toBe(field);
    fireEvent.change(field, { target: { value: " Fix it now " } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(onSave).toHaveBeenCalledWith("Fix it now"));
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole("button", { name: "Field Fix it" })),
    );
  });

  it("leaves on Escape without saving, and Escape goes no further", async () => {
    const onSave = saving();
    const outside = vi.fn();
    render(
      // biome-ignore lint/a11y/noStaticElementInteractions: listens for what reaches the dock behind the property
      <div onKeyDown={outside}>
        <EditableProperty name="Field" editor={{ kind: "text", value: "Fix it", onSave }}>
          Fix it
        </EditableProperty>
      </div>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Field Fix it" }));
    const field = screen.getByRole("textbox", { name: "Field" });
    fireEvent.change(field, { target: { value: "Something else" } });
    fireEvent.keyDown(field, { key: "Escape" });
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(outside).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole("button", { name: "Field Fix it" })),
    );
    expect(onSave).not.toHaveBeenCalled();
  });

  it("saves when the field is left, and not when nothing changed", async () => {
    const onSave = saving();
    property({ kind: "text", value: "Fix it", onSave }, "Fix it");
    fireEvent.click(screen.getByRole("button", { name: "Field Fix it" }));
    fireEvent.blur(screen.getByRole("textbox", { name: "Field" }));
    expect(onSave).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Field Fix it" }));
    const field = screen.getByRole("textbox", { name: "Field" });
    fireEvent.change(field, { target: { value: "Fixed" } });
    fireEvent.blur(field);
    await waitFor(() => expect(onSave).toHaveBeenCalledWith("Fixed"));
  });

  it("reads a number, with or without #, and an empty field as none", async () => {
    const onSave = saving();
    property({ kind: "number", value: 3, onSave }, "#3");
    const open = () => {
      fireEvent.click(screen.getByRole("button", { name: "Field #3" }));
      return screen.getByRole("textbox", { name: "Field" });
    };
    let field = open();
    expect((field as HTMLInputElement).value).toBe("3");
    fireEvent.change(field, { target: { value: "7" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(onSave).toHaveBeenLastCalledWith(7));
    field = open();
    fireEvent.change(field, { target: { value: "" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(onSave).toHaveBeenLastCalledWith(null));
  });

  it("sets a date, and clears it", async () => {
    const onSave = saving();
    const { container } = property({ kind: "date", value: null, onSave }, "No date");
    fireEvent.click(screen.getByRole("button", { name: "Field No date" }));
    const field = container.querySelector('input[type="date"]') as HTMLInputElement;
    expect(field.getAttribute("aria-labelledby")).toBeTruthy();
    fireEvent.change(field, { target: { value: "2026-12-01" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(onSave).toHaveBeenCalledWith("2026-12-01"));
  });

  it("says the refusal under the property, as the value's description, and clears it with the next save", async () => {
    let refuse = true;
    const onSave = vi.fn(async () => {
      if (refuse) throw new Error("the title is blank");
    });
    property({ kind: "text", value: "Fix it", onSave }, "Fix it");
    fireEvent.click(screen.getByRole("button", { name: "Field Fix it" }));
    let field = screen.getByRole("textbox", { name: "Field" });
    fireEvent.change(field, { target: { value: " " } });
    fireEvent.keyDown(field, { key: "Enter" });
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("the title is blank");
    const value = screen.getByRole("button", { name: "Field Fix it" });
    expect(description(value)).toContain("the title is blank");

    refuse = false;
    fireEvent.click(value);
    field = screen.getByRole("textbox", { name: "Field" });
    fireEvent.change(field, { target: { value: "Fixed" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(description(screen.getByRole("button", { name: "Field Fix it" }))).toBe("");
  });

  it("waits for one save before it opens again, and says it is busy", async () => {
    let finish = () => {};
    const onSave = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
    property({ kind: "text", value: "Fix it", onSave }, "Fix it");
    fireEvent.click(screen.getByRole("button", { name: "Field Fix it" }));
    const field = screen.getByRole("textbox", { name: "Field" });
    fireEvent.change(field, { target: { value: "Fixed" } });
    fireEvent.keyDown(field, { key: "Enter" });
    const value = screen.getByRole("button", { name: "Field Fix it" });
    await waitFor(() => expect(value.getAttribute("aria-busy")).toBe("true"));
    expect(value.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(value);
    expect(screen.queryByRole("textbox")).toBeNull();
    await act(async () => finish());
    await waitFor(() => expect(value.getAttribute("aria-busy")).toBeNull());
  });
});

describe("a property of several values", () => {
  function labels(onAdd = saving(), onRemove = saving()) {
    render(
      <ListProperty
        name="Labels"
        items={["bug", "ui"].map((name) => ({ key: name, label: name, remove: `Remove ${name}` }))}
        empty="None"
        addLabel="Add a label"
        onAdd={onAdd}
        onRemove={onRemove}
      />,
    );
    return { onAdd, onRemove };
  }

  it("removes one value with its own button, and the focus goes on to the add button", async () => {
    const { onRemove } = labels();
    const remove = screen.getByRole("button", { name: "Remove bug" });
    remove.focus();
    fireEvent.click(remove);
    await waitFor(() => expect(onRemove).toHaveBeenCalledWith("bug"));
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Add a label" }));
  });

  it("adds one value from a field opened by its button, with Enter", async () => {
    const { onAdd } = labels();
    fireEvent.click(screen.getByRole("button", { name: "Add a label" }));
    const field = screen.getByRole("textbox", { name: "Add a label" });
    expect(document.activeElement).toBe(field);
    fireEvent.change(field, { target: { value: " docs " } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(onAdd).toHaveBeenCalledWith("docs"));
  });

  it("says a refusal under the list, and describes its buttons with it", async () => {
    labels(
      vi.fn(async () => {
        throw new Error("no such label in the repository");
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Add a label" }));
    const field = screen.getByRole("textbox", { name: "Add a label" });
    fireEvent.change(field, { target: { value: "docs" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect((await screen.findByRole("alert")).textContent).toContain("no such label");
    expect(description(screen.getByRole("button", { name: "Add a label" }))).toContain(
      "no such label",
    );
  });

  it("says what it has when it has nothing", () => {
    render(
      <ListProperty
        name="Labels"
        items={[]}
        empty="None"
        addLabel="Add a label"
        onAdd={saving()}
        onRemove={saving()}
      />,
    );
    expect(screen.getByText("None")).toBeTruthy();
  });
});
