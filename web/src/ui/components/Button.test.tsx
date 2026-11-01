// @vitest-environment jsdom
/**
 * A button that cannot be pressed says why in the one place that reaches everyone: the words a screen reader is told
 * alongside its name. Printing them beside the control would be furniture, and handing them to the browser's `title`
 * draws a box in colours nobody chose over a page that has its own.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Button } from "./Button";

afterEach(cleanup);

const REASON = "Only one change can be offered at a time.";

describe("a button that cannot be pressed", () => {
  it("keeps the reason out of the browser's own tooltip box", () => {
    render(
      <Button disabled title={REASON}>
        Offer the change
      </Button>,
    );
    // The box a `title` draws is drawn by the browser, in its own colours, over a page that has its own: a reason
    // worth giving is worth giving in the accessibility tree, where it reaches a keyboard too.
    expect(
      screen.getByRole("button", { name: "Offer the change" }).getAttribute("title"),
    ).toBeNull();
  });

  it("tells a screen reader the reason without printing it beside the button", () => {
    render(
      <Button disabled title={REASON}>
        Offer the change
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Offer the change" });
    const describedBy = button.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    const quiet = document.getElementById(describedBy ?? "");
    expect(quiet?.textContent).toBe(REASON);
    expect(quiet?.getAttribute("class")).toContain("sr-only");
    // The reason is written once, in the line nobody sees: not in the name, not beside the control.
    expect(screen.getAllByText(REASON)).toHaveLength(1);
    expect(button.textContent).toBe("Offer the change");
  });

  it("keeps whatever else the button was already pointed at", () => {
    render(
      <>
        <p id="waiting">The run has to end first.</p>
        <Button disabled title={REASON} aria-describedby="waiting">
          Offer the change
        </Button>
      </>,
    );
    const describedBy =
      screen.getByRole("button", { name: "Offer the change" }).getAttribute("aria-describedby") ??
      "";
    const [hint, reason] = describedBy.split(" ");
    expect(document.getElementById(hint ?? "")?.textContent).toBe("The run has to end first.");
    expect(document.getElementById(reason ?? "")?.textContent).toBe(REASON);
  });
});

describe("a button that can be pressed", () => {
  it("has nothing to explain", () => {
    render(<Button title="The change is ready to merge">Offer the change</Button>);
    const button = screen.getByRole("button", { name: "Offer the change" });
    expect(button.getAttribute("title")).toBe("The change is ready to merge");
    expect(button.hasAttribute("aria-describedby")).toBe(false);
    expect(document.querySelector(".sr-only")).toBeNull();
  });

  it("adds nothing of its own when it has no reason to give", () => {
    render(
      <Button disabled>
        <span className="sr-only">Loading</span>
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Loading" });
    expect(button.hasAttribute("aria-describedby")).toBe(false);
    // The only quiet words are the caller's own.
    expect(document.querySelectorAll(".sr-only")).toHaveLength(1);
  });
});

describe("a button inside a form", () => {
  // A `<button>` with no `type` is a submit button, which is the wrong default for the one control whose whole job
  // is not doing the thing the form does: cancelling an issue used to create it, and the dialog closed as if it had
  // worked. Only a button that says it submits may submit.
  const submits = () => {
    render(
      <form
        onSubmit={(event) => {
          event.preventDefault();
          submitted.push("submitted");
        }}
      >
        <Button variant="ghost">Cancel</Button>
        <Button type="submit">Create</Button>
      </form>,
    );
  };
  let submitted: string[] = [];

  afterEach(() => {
    submitted = [];
  });

  it("does not submit the form when it is a plain button", () => {
    submits();
    screen.getByRole("button", { name: "Cancel" }).click();
    expect(submitted).toEqual([]);
  });

  it("submits the form only when it was told to", () => {
    submits();
    screen.getByRole("button", { name: "Create" }).click();
    expect(submitted).toEqual(["submitted"]);
  });
});
