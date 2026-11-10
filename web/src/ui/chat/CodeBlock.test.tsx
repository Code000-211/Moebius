// @vitest-environment jsdom
/**
 * A block of code: its head says which language it came in as and carries the Copy button, its body is highlighted by
 * the shared grammars once the lazy module arrives (plain until then). A language the map does not know keeps its
 * name in the head but is drawn plain; a block that names no language is labeled "Plain text". Copy always copies
 * exactly what was written.
 */
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CodeBlock } from "./CodeBlock";

afterEach(cleanup);

describe("CodeBlock", () => {
  it("says the language in its head and keeps a Copy button for the plain text", () => {
    const { getByText, getByRole } = render(<CodeBlock language="rust" code={"fn main() {}"} />);
    expect(getByText("rust").tagName).toBe("SPAN");
    expect(getByRole("button", { name: "Copy code" }).tagName).toBe("BUTTON");
  });

  it("highlights an answer's code by the palette's classes", async () => {
    const { container } = render(
      <CodeBlock language="rust" code={'fn main() {\n    println!("Answer");\n}\n'} />,
    );
    await waitFor(() => expect(container.querySelector(".tok-keyword")?.textContent).toBe("fn"));
    expect(
      [...container.querySelectorAll(".tok-title")].some((node) => node.textContent === "println"),
    ).toBe(true);
    expect(container.querySelector("pre")?.textContent).toContain('println!("Answer");');
  });

  it("labels a language it does not know with its name and draws it plain", async () => {
    const { container, getByText } = render(<CodeBlock language="whpl" code="const x = 1" />);
    await waitFor(() => expect(getByText("whpl").tagName).toBe("SPAN"));
    await waitFor(() => expect(container.querySelector("code")?.textContent).toBe("const x = 1"));
    // The highlight module has arrived by now, so plain here means the parser said no, not that it has not loaded.
    expect(container.querySelector(".tok-keyword")).toBeNull();
  });

  it("labels a block that names no language Plain text", () => {
    const { getByText, getByRole } = render(<CodeBlock code="const x = 1" />);
    expect(getByText("Plain text").tagName).toBe("SPAN");
    expect(getByRole("button", { name: "Copy code" }).tagName).toBe("BUTTON");
  });
});
