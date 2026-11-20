// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Input } from "../components/Input";
import { FieldRow, Rows } from "./Field";

afterEach(cleanup);

describe("a field row", () => {
  it("names the control by its label and describes it by the description", () => {
    render(
      <Rows>
        <FieldRow label="Command" description="Default pi-bolt">
          <Input defaultValue="x" />
        </FieldRow>
      </Rows>,
    );
    const control = screen.getByLabelText("Command");
    expect(control.getAttribute("aria-describedby")).toBeTruthy();
    expect(screen.getByText("Default pi-bolt").id).toBe(control.getAttribute("aria-describedby"));
  });

  it("shows an error in place of the description, under the control", () => {
    render(
      <FieldRow label="Runs" description="a fact" error="Whole number, please">
        <Input defaultValue="x" />
      </FieldRow>,
    );
    expect(screen.queryByText("a fact")).toBeNull();
    const control = screen.getByLabelText("Runs");
    expect(screen.getByText("Whole number, please").id).toBe(
      control.getAttribute("aria-describedby"),
    );
  });
});
