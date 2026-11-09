/** The form of an `approval` node: the prompt a person reads and how long the run waits for them. */

import { Input } from "../../../ui/components/Input";
import { Field, TextArea } from "../../../ui/page/Field";
import type { NodeFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";

export function ApprovalForm({ node, onParams }: NodeFormProps) {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const prompt = typeof params.prompt === "string" ? params.prompt : "";
  const timeout = typeof params.timeout_seconds === "number" ? String(params.timeout_seconds) : "";
  const missing = prompt.trim() === "";

  function give(name: string, value: unknown) {
    const next = { ...params };
    if (value === undefined || value === "") {
      delete next[name];
    } else {
      next[name] = value;
    }
    onParams(next);
  }

  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="approval-form">
      <Field
        label="Prompt"
        hint="What the person deciding reads. Expressions such as {{ trigger.tag }} are filled before it shows."
        error={missing ? "An approval node needs the prompt a person reads." : undefined}
      >
        <TextArea
          aria-label="Prompt"
          rows={5}
          value={prompt}
          onChange={(event) => give("prompt", event.target.value)}
          placeholder="Publish 1.4.0 to the registry?"
        />
      </Field>
      <Field
        label="Deadline"
        hint="How long the run waits for an answer, in seconds. Left alone, the kind's own deadline."
      >
        <Input
          type="number"
          inputMode="numeric"
          min={1}
          aria-label="Deadline"
          value={timeout}
          onChange={(event) => {
            const value = event.target.value;
            give("timeout_seconds", value.trim() === "" ? undefined : Number(value));
          }}
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}
