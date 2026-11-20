/**
 * The key of a provider is rows of the area, not a box: each control is named by its own label at the left, and the
 * rows that depend on where the key comes from appear and go with the choice.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { KeyField } from "./KeyField";
import type { KeyChoice } from "./model";

const choice = (patch: Partial<KeyChoice>): KeyChoice => ({
  mode: "none",
  env: "",
  typed: "",
  stored: null,
  ...patch,
});

describe("the key rows", () => {
  it("shows one row for no key, with the fact beside its label", () => {
    render(
      <KeyField label="Key of provider 1" choice={choice({})} secrets={[]} onChange={() => {}} />,
    );
    expect(screen.getByText("Key")).toBeTruthy();
    expect(screen.getByText(/No key: the address is open/)).toBeTruthy();
    expect(screen.queryByLabelText("Key of provider 1: environment variable")).toBeNull();
  });

  it("adds the row of the variable for an environment key", () => {
    render(
      <KeyField
        label="Key of provider 1"
        choice={choice({ mode: "env", env: "ANTHROPIC_API_KEY" })}
        secrets={[]}
        onChange={() => {}}
      />,
    );
    const field = screen.getByLabelText("Key of provider 1: environment variable");
    expect((field as HTMLInputElement).value).toBe("ANTHROPIC_API_KEY");
    expect(screen.getByText("Environment variable")).toBeTruthy();
  });

  it("adds the key and the delete rows for a stored key", () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <KeyField
          label="Key of provider 1"
          choice={choice({ mode: "typed", stored: "main-key" })}
          secrets={["main-key"]}
          onChange={() => {}}
        />
      </QueryClientProvider>,
    );
    expect(screen.getByLabelText("Key of provider 1: the key")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Delete stored key" })).toBeTruthy();
  });
});
