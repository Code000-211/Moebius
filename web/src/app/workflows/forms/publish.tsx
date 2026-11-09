/** The form of a `publish` node: the title and body of what it publishes. */

import { Input } from "../../../ui/components/Input";
import { Field, TextArea } from "../../../ui/page/Field";
import type { NodeFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";

export function PublishForm({ node, onParams }: NodeFormProps) {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const title = typeof params.title === "string" ? params.title : "";
  const body = typeof params.body === "string" ? params.body : "";
  const missing = title.trim() === "";

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
    <div className="flex min-w-0 flex-col gap-4" data-testid="publish-form">
      <Field
        label="Title"
        hint="The title of what the node publishes. Expressions such as {{ trigger.tag }} are filled before it does."
        error={missing ? "A publish node needs its title." : undefined}
      >
        <Input
          aria-label="Title"
          value={title}
          onChange={(event) => give("title", event.target.value)}
          placeholder="Release 1.4.0"
        />
      </Field>
      <Field label="Body" hint="The body that goes with it.">
        <TextArea
          aria-label="Body"
          rows={8}
          value={body}
          onChange={(event) => give("body", event.target.value)}
          placeholder="What changed in this release."
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}
