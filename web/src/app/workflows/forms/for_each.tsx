/** The form of a `for_each` node: where its sub-path rejoins, and how far it may fan out. */

import { Input } from "../../../ui/components/Input";
import { Field, Select } from "../../../ui/page/Field";
import type { NodeFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";

export function ForEachForm({ node, graph, onParams }: NodeFormProps) {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const rejoin = typeof params.rejoin === "string" ? params.rejoin : "";
  const maxElements = typeof params.max_elements === "number" ? String(params.max_elements) : "";
  const missing = rejoin === "";

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
    <div className="flex min-w-0 flex-col gap-4" data-testid="for-each-form">
      <Field
        label="Rejoins at"
        hint="The node the sub-path comes back at; its output is what each element gives back. Expressions such as {{ trigger.tag }} may be typed where the file takes a value."
        error={missing ? "A for-each node needs the node its sub-path rejoins at." : undefined}
      >
        <Select
          aria-label="Rejoins at"
          value={rejoin}
          onChange={(value) => give("rejoin", value)}
          options={[
            ["", "Choose a node"],
            ...graph.nodes
              .filter((other) => other.id !== node.id)
              .map((other): [string, string] => [other.id, other.id]),
          ]}
        />
      </Field>
      <Field
        label="Most elements"
        hint="The most elements one run of the node may fan out over. Left alone, a hundred."
      >
        <Input
          type="number"
          inputMode="numeric"
          min={0}
          aria-label="Most elements"
          value={maxElements}
          onChange={(event) => {
            const value = event.target.value;
            give("max_elements", value.trim() === "" ? undefined : Number(value));
          }}
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}
