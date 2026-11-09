/** The form of a `code` node: the Luau it runs, which may hold expressions like any parameter. */

import { Field, TextArea } from "../../../ui/page/Field";
import type { NodeFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";

export function CodeForm({ node, onParams }: NodeFormProps) {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const source = typeof params.source === "string" ? params.source : "";
  const missing = source.trim() === "";

  function changeSource(value: string) {
    const next = { ...params };
    if (value === "") {
      delete next.source;
    } else {
      next.source = value;
    }
    onParams(next);
  }

  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="code-form">
      <Field
        label="Source"
        hint={
          source === "" ? (
            "Luau that gets the value as `input` and returns what the node gives back."
          ) : (
            <>
              Luau that gets the value as <span className="font-mono">input</span> and returns what
              the node gives back. Expressions such as{" "}
              <span className="font-mono">{"{{ trigger.tag }}"}</span> are filled before it runs.
            </>
          )
        }
        error={missing ? "A code node needs its source." : undefined}
      >
        <TextArea
          aria-label="Source"
          value={source}
          onChange={(event) => changeSource(event.target.value)}
          rows={10}
          mono
          placeholder="return input"
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}
