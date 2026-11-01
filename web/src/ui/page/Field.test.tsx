// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Input } from "../components/Input";
import { Field, Select } from "./Field";

afterEach(cleanup);

/**
 * A field explains itself under the control, but the explanation must not become part of the control's name: a person
 * hearing the page should hear "Command, edit" and then the hint, not the hint folded into the name.
 */
describe("a field with a hint", () => {
  it("names the control by its label alone", () => {
    render(
      <Field label="Command" hint="The program the server starts.">
        <Input defaultValue="pi-bolt" />
      </Field>,
    );
    expect(screen.getByLabelText("Command")).toBeTruthy();
    expect(screen.getByRole("textbox").getAttribute("aria-label")).toBeNull();
  });

  it("points the control at the line that explains it", () => {
    render(
      <Field label="Command" hint="The program the server starts.">
        <Input defaultValue="pi-bolt" />
      </Field>,
    );
    const control = screen.getByRole("textbox");
    const describedBy = control.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    expect(document.getElementById(describedBy ?? "")?.textContent).toBe(
      "The program the server starts.",
    );
  });

  it("names the line that goes wrong with the same words a person reads", () => {
    render(
      <Field label="Path" hint="Relative to the skill.">
        <Input defaultValue="notes/a.md" />
      </Field>,
    );
    expect(screen.getByRole("textbox").getAttribute("aria-describedby")).toBeTruthy();
    expect(screen.getByText("Relative to the skill.")).toBeTruthy();
  });
});

/**
 * A select is the same control as a text field, drawn with a chooser: it keeps the shared classes rather than being
 * replaced by the browser's own small widget.
 */
describe("a choice in a field", () => {
  it("keeps the classes of a control", () => {
    render(
      <Field label="Kind" className="max-w-xs">
        {(id) => (
          <Select
            id={id}
            options={["stdio", "http"]}
            value="stdio"
            onChange={() => {}}
            className="w-auto"
          />
        )}
      </Field>,
    );
    const control = screen.getByRole("button", { name: "Kind" });
    expect(control.className).toContain("min-h-10");
    expect(control.className).toContain("rounded-control");
    // The caller's own classes ride along; they do not replace the control's.
    expect(control.className).toContain("w-auto");
  });

  /**
   * A choice opens this app's menu rather than the list the operating system draws, so the rows are the same 40px
   * rows as every other list and are coloured by the theme. The list is not there until it is opened, and the trigger
   * says what stands now.
   */
  it("keeps the choices in the app's own menu", async () => {
    render(
      <Select aria-label="Kind" options={["stdio", "http"]} value="stdio" onChange={() => {}} />,
    );
    const control = screen.getByRole("button", { name: "Kind" });
    expect(control.getAttribute("options")).toBeNull();
    expect(control.getAttribute("aria-haspopup")).toBe("menu");
    expect(screen.queryAllByRole("menuitemradio")).toHaveLength(0);
    // The menu opens on the pointer going down on the trigger, the way a person opens it.
    fireEvent.pointerDown(control, { button: 0, ctrlKey: false });
    expect(await screen.findAllByRole("menuitemradio")).toHaveLength(2);
    expect(screen.getAllByRole("menuitemradio").map((each) => each.textContent?.trim())).toEqual([
      "stdio",
      "http",
    ]);
    // The one that stands is a radio in the group, so it is said as chosen rather than as a plain row.
    expect(
      screen.getAllByRole("menuitemradio").map((each) => each.getAttribute("aria-checked")),
    ).toEqual(["true", "false"]);
  });

  it("shows what stands now", () => {
    render(
      <Select aria-label="Kind" options={["stdio", "http"]} value="http" onChange={() => {}} />,
    );
    expect(screen.getByRole("button", { name: "Kind" }).textContent).toContain("http");
  });

  /**
   * A select sits inside the `<label>` of its field, so without an explicit name it is the field's label that names
   * it — "Kind" — rather than the words of every choice it holds.
   */
  it("is named by the field, not by its own choices", () => {
    render(
      <Field label="Kind" hint="How the server is reached.">
        {(id) => <Select id={id} options={["stdio", "http"]} value="stdio" onChange={() => {}} />}
      </Field>,
    );
    expect(screen.getByLabelText("Kind", { exact: true })).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Kind" }).getAttribute("aria-labelledby"),
    ).toBeTruthy();
  });

  /**
   * One card often holds several upstreams, where the field's label — "Key" — says which part of the card a control
   * is, not which control it is. The caller's own name has to survive the field, because `aria-labelledby` would
   * otherwise win over it and every upstream would answer to the same words.
   */
  it("keeps a name the caller gave it", () => {
    render(
      <Field label="Key" hint="Name the variable the server reads.">
        {(id) => (
          <Select
            id={id}
            aria-label="Key of upstream 3: where it comes from"
            options={["none", "env"]}
            value="env"
            onChange={() => {}}
          />
        )}
      </Field>,
    );
    const control = screen.getByRole("button", {
      name: "Key of upstream 3: where it comes from",
    });
    expect(control.getAttribute("aria-labelledby")).toBeNull();
    // The hint still explains it.
    expect(control.getAttribute("aria-describedby")).toBeTruthy();
  });
});
