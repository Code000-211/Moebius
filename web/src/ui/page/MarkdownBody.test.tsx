// @vitest-environment jsdom
/**
 * The choice of how to read a Markdown file. It opens as the text, which is what a person came to look at, and the
 * drawing is one tab away. The choice is held by whoever shows the file, so it survives moving to another one, and
 * swapping the two views does not lose what is typed. A file with nothing in it says so rather than drawing a blank
 * panel.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { MarkdownBody, type MarkdownView } from "./MarkdownBody";

const text = "# Plain\n\nWrite in short sentences.";

afterEach(cleanup);

/** A page that shows one Markdown file and holds how it is read, as SkillEditor does. */
function Page({ file = text, readOnly = false }: { file?: string; readOnly?: boolean }) {
  const [view, setView] = useState<MarkdownView>("raw");
  return (
    <MarkdownBody
      label="SKILL.md"
      text={file}
      readOnly={readOnly}
      onChange={readOnly ? undefined : () => {}}
      view={view}
      onViewChange={setView}
    />
  );
}

describe("the body of a Markdown file", () => {
  it("opens as the text, with the drawing one tab away", () => {
    render(<Page />);
    expect(screen.getByRole("textbox", { name: "SKILL.md" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Plain" })).toBeNull();
    fireEvent.click(screen.getByRole("tab", { name: "Rendered" }));
    expect(screen.getByRole("heading", { name: "Plain" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Rendered" }).getAttribute("aria-selected")).toBe(
      "true",
    );
  });

  it("opens a file that cannot be changed as the text too, read only", () => {
    render(<Page readOnly />);
    const box = screen.getByRole("textbox", { name: "SKILL.md" }) as HTMLTextAreaElement;
    expect(box.readOnly).toBe(true);
    expect(box.value).toBe(text);
  });

  it("is read the same way when another file is shown", () => {
    const first = render(<Page />);
    fireEvent.click(screen.getByRole("tab", { name: "Rendered" }));
    // Picking another file of the same skill changes the text, not how it is read.
    first.rerender(<Page file="# Other" />);
    expect(screen.getByRole("heading", { name: "Other" })).toBeTruthy();
    expect(screen.queryByRole("textbox", { name: "SKILL.md" })).toBeNull();
  });

  it("swaps the text for the drawing and back without losing the text", () => {
    render(<Page />);
    fireEvent.click(screen.getByRole("tab", { name: "Rendered" }));
    fireEvent.click(screen.getByRole("tab", { name: "Raw" }));
    expect((screen.getByRole("textbox", { name: "SKILL.md" }) as HTMLTextAreaElement).value).toBe(
      text,
    );
  });

  it("says that a file has nothing in it rather than drawing nothing", () => {
    render(<Page file="" readOnly />);
    fireEvent.click(screen.getByRole("tab", { name: "Rendered" }));
    expect(screen.getByText("Nothing written yet.")).toBeTruthy();
  });
});
