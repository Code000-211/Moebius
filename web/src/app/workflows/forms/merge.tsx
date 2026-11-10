/** The form of a `merge` node: the two branches it joins, and whether both may arrive. */

import { Field, Select } from "../../../ui/page/Field";
import { Switch } from "../../../ui/page/Switch";
import type { NodeFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";

export function MergeForm({ node, graph, onParams }: NodeFormProps) {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const a = typeof params.a === "string" ? params.a : "";
  const b = typeof params.b === "string" ? params.b : "";
  const both = params.both === true;
  const missing =
    a === "" || b === ""
      ? "A merge node needs the two nodes whose values arrive by its sides."
      : a === b
        ? "The two sides of a merge must be different nodes."
        : undefined;

  function give(name: string, value: unknown) {
    const next = { ...params };
    if (value === undefined || value === "" || value === false) {
      delete next[name];
    } else {
      next[name] = value;
    }
    onParams(next);
  }

  function choices(other: string): [string, string][] {
    return [
      ["", "Choose a node"],
      ...graph.nodes
        .filter((entry) => entry.id !== node.id && entry.id !== other)
        .map((entry): [string, string] => [entry.id, entry.id]),
    ];
  }

  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="merge-form">
      <Field
        label="Side A"
        hint="The node whose value arrives by the merge's `a` side."
        error={missing && a === "" ? missing : undefined}
      >
        <Select
          aria-label="Side A"
          value={a}
          onChange={(value) => give("a", value)}
          options={choices(b)}
        />
      </Field>
      <Field
        label="Side B"
        hint="The node whose value arrives by the merge's `b` side."
        error={missing && b === "" ? missing : undefined}
      >
        <Select
          aria-label="Side B"
          value={b}
          onChange={(value) => give("b", value)}
          options={choices(a)}
        />
      </Field>
      <Field
        label="Both sides"
        hint="Emit every side that arrived, in the order they arrived, not only one."
      >
        <Switch checked={both} onChange={(next) => give("both", next)} label="Both sides" />
      </Field>
      {a !== "" && b !== "" && a === b ? (
        <p className="m-0 text-sm text-error-text" data-testid="merge-problem">
          {missing}
        </p>
      ) : null}
      <ParamsPreview params={params} />
    </div>
  );
}
