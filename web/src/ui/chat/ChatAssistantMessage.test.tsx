// @vitest-environment jsdom
/**
 * An answer is its words and nothing else. The Copy button that comes with it belongs to the same block but does not
 * sit on a row under the last line: a row that waits to be hovered is invisible and 32px tall at the same time, so
 * every answer ended in a band of nothing. The row is out of the flow instead, in the corner the words stop at
 * (`copy-float`), and the block is what it is measured against and what shows it on hover.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ChatAssistantMessage } from "./ChatAssistantMessage";
import { CopyButton } from "./CopyButton";

afterEach(cleanup);

function drawn() {
  return (
    <ChatAssistantMessage
      content="the answer"
      renderMarkdown={({ content }) => <p>{content}</p>}
      actions={<CopyButton text="the answer" label="Copy message" />}
    />
  );
}

describe("an answer with a Copy button", () => {
  it("puts the row in the corner of the words rather than under them", () => {
    render(drawn());
    const row = screen.getByRole("button", { name: "Copy message" }).parentElement;
    expect(row?.getAttribute("class")).toContain("copy-float");
    // The block is what the row is positioned against, and is the thing a pointer is over to bring it back.
    const block = row?.parentElement;
    expect(block?.getAttribute("class")).toContain("relative");
    expect(block?.getAttribute("class")).toContain("reveal-copy");
  });
});
