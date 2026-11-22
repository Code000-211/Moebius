// @vitest-environment jsdom
/** Long lines wrap in a code block and a diff when the person chose it, and run on, scrolling sideways, when not. */
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CodeBlock } from "../ui/chat/CodeBlock";
import { DiffBody } from "../ui/chat/DiffBody";
import { WrapProvider } from "./wrapping";

afterEach(cleanup);

const LONG = `const long = "${"x".repeat(300)}";`;

describe("wrapping long lines", () => {
  it("lets a code block scroll sideways by default", async () => {
    const { container } = render(<CodeBlock language="ts" code={LONG} />);
    const pre = container.querySelector("pre") as HTMLElement;
    expect(pre.className).toContain("overflow-x-auto");
    expect(pre.querySelector("code")?.className).toContain("whitespace-pre");
    expect(pre.querySelector("code")?.className).not.toContain("whitespace-pre-wrap");
  });

  it("wraps a code block, highlighted or not yet, when the person chose it", async () => {
    const { container, findByText } = render(
      <WrapProvider value={true}>
        <CodeBlock language="ts" code={LONG} />
      </WrapProvider>,
    );
    const pre = container.querySelector("pre") as HTMLElement;
    expect(pre.className).toContain("overflow-x-hidden");
    expect(pre.className).not.toContain("overflow-x-auto");
    expect(pre.querySelector("code")?.className).toContain("whitespace-pre-wrap");
    // Once the grammar has loaded the highlighted code wraps as well.
    await findByText(/xxxx/);
    await new Promise((done) => setTimeout(done, 0));
    expect(container.querySelector("pre code")?.className).toContain("whitespace-pre-wrap");
  });

  it("wraps the lines of a diff when the person chose it", () => {
    const changes = [{ oldText: "old", newText: LONG }];
    const off = render(<DiffBody changes={changes} />);
    expect(off.container.querySelector("pre")?.className).toContain("overflow-x-auto");
    off.unmount();
    const on = render(
      <WrapProvider value={true}>
        <DiffBody changes={changes} />
      </WrapProvider>,
    );
    expect(on.container.querySelector("pre")?.className).toContain("overflow-x-hidden");
    expect(on.container.querySelector("pre div")?.className).toContain("whitespace-pre-wrap");
  });
});
