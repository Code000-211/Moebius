/**
 * The choice in the composer says what stands now, including when the answer is the server's rather than a pick: an
 * agent with no model of its own has an empty model, not a missing one, and the trigger used to read "Model:" with
 * nothing after it. What stands is also what a screen reader hears, so the words belong to the trigger's name.
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PillSelect } from "./PillSelect";

const INHERITED = { value: "", label: "Server default", inherited: true };

describe("what a choice shows", () => {
  it("names the model an agent brings rather than nothing", () => {
    render(
      <PillSelect
        label="Model"
        value=""
        placeholder="Model"
        options={[INHERITED, { value: "fake-small", label: "fake-small" }]}
        onChange={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: "Model: Server default" })).not.toBeNull();
  });

  it("falls back to its placeholder when there is nothing to show", () => {
    render(
      <PillSelect
        label="Model"
        value=""
        placeholder="Server default"
        options={[{ value: "", label: "" }]}
        onChange={() => {}}
      />,
    );
    const trigger = screen.getByRole("button", { name: "Model: Server default" });
    expect(trigger.textContent).toContain("Server default");
  });

  it("shows the model that was chosen, not the one that was inherited", () => {
    render(
      <PillSelect
        label="Model"
        value="fake-small"
        placeholder="Model"
        options={[INHERITED, { value: "fake-small", label: "fake-small" }]}
        onChange={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: "Model: fake-small" })).not.toBeNull();
  });
});
