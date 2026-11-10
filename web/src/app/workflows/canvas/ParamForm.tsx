/** The parameters a node is given, in a form built from what its kind reports. */
import { useState } from "react";
import type { WorkflowDefinition, WorkflowNode } from "../../../api/types/workflows";
import { Input } from "../../../ui/components/Input";
import { Field, Select, TextArea } from "../../../ui/page/Field";
import { Switch } from "../../../ui/page/Switch";
import type { ParamField } from "../nodeKinds";

/**
 * The generic parameter form: one control per field the kind reports, so a kind that lands later needs no change
 * here. A parameter the fields do not describe is kept and shown as JSON below them, and a kind the registry does not
 * know at all is edited as JSON whole — the canvas never rewrites parameters it was not asked about.
 *
 * While a parameter is being typed it lives here, because a half-written number, or a half-typed list of lines, is not
 * a parameter yet. Whoever shows the form keys it by the node, so another node's parameters start the controls over.
 */
export function ParamForm({
  node,
  graph,
  fields,
  known,
  onParams,
}: {
  /** The node whose parameters are shown. */
  node: WorkflowNode;
  /** The graph it is on, for a parameter that names another node. */
  graph: WorkflowDefinition;
  /** What its kind reports, or nothing at all when the registry does not know the kind. */
  fields: ParamField[];
  /** Whether the registry knows the kind at all. */
  known: boolean;
  /** What to do with the parameters, once one of them has changed. */
  onParams: (params: Record<string, unknown>) => void;
}) {
  const described = new Set(fields.map((field) => field.name));
  const params = (node.params ?? {}) as Record<string, unknown>;
  const extras = Object.keys(params).filter((name) => !described.has(name));
  const missing = fields
    .filter((field) => "required" in field && field.required)
    .map((field) => field.name)
    .filter((name) => params[name] === undefined || params[name] === "");

  const [drafts, setDrafts] = useState<Record<string, string>>(() => draftsOf(params, fields));
  const [extraDraft, setExtraDraft] = useState(() => jsonOf(pick(params, extras)));
  const [extraBroken, setExtraBroken] = useState(false);

  function give(name: string, value: unknown) {
    const next = { ...params };
    if (value === undefined) {
      delete next[name];
    } else {
      next[name] = value;
    }
    onParams(next);
  }

  function keep(name: string, value: string, then: (value: string) => unknown) {
    setDrafts((current) => ({ ...current, [name]: value }));
    give(name, then(value));
  }

  function changeExtras(value: string) {
    setExtraDraft(value);
    try {
      const parsed = JSON.parse(value) as Record<string, unknown>;
      setExtraBroken(false);
      const kept = { ...params };
      for (const name of extras) {
        delete kept[name];
      }
      onParams({ ...kept, ...parsed });
    } catch {
      setExtraBroken(true);
    }
  }

  const others = graph.nodes.filter((other) => other.id !== node.id);
  const extraLabel = known ? "Other parameters" : "Parameters";

  return (
    <div className="flex min-w-0 flex-col gap-4">
      {known ? null : (
        <p className="text-sm text-text-secondary">
          Nothing in the registry describes <span className="font-mono">{node.kind}</span> yet.
        </p>
      )}
      {fields.map((field) => (
        <ParamControl
          key={field.name}
          field={field}
          value={params[field.name]}
          draft={drafts[field.name] ?? ""}
          others={others}
          onText={(value) => keep(field.name, value, (v) => (v === "" ? undefined : v))}
          onNumber={(value) =>
            keep(field.name, value, (v) => (v.trim() === "" ? undefined : Number(v)))
          }
          onLines={(value) =>
            keep(field.name, value, (v) => {
              const list = v
                .split("\n")
                .map((line) => line.trim())
                .filter((line) => line !== "");
              return list.length === 0 ? undefined : list;
            })
          }
          onJson={(value) => {
            setDrafts((current) => ({ ...current, [field.name]: value }));
            try {
              give(field.name, JSON.parse(value) as unknown);
            } catch {
              // Half-written JSON is not a parameter yet; what was there stays.
            }
          }}
          onPicked={(value) => give(field.name, value === "" ? undefined : value)}
          onSwitched={(value) => give(field.name, value)}
        />
      ))}
      {missing.length === 0 ? null : (
        <p className="text-sm text-attention-text">
          {missing.map((name) => `"${name}"`).join(", ")} still has no value, and this kind needs
          it.
        </p>
      )}
      {known && extras.length === 0 ? null : (
        <Field
          label={extraLabel}
          hint="The parameters as the file writes them, kept as they are."
          error={extraBroken ? "This is not JSON yet." : undefined}
        >
          <TextArea
            aria-label={extraLabel}
            value={extraDraft}
            onChange={(event) => changeExtras(event.target.value)}
            rows={5}
            mono
            className="min-h-24"
          />
        </Field>
      )}
    </div>
  );
}

function ParamControl({
  field,
  value,
  draft,
  others,
  onText,
  onNumber,
  onLines,
  onJson,
  onPicked,
  onSwitched,
}: {
  field: ParamField;
  value: unknown;
  draft: string;
  others: WorkflowNode[];
  onText: (value: string) => void;
  onNumber: (value: string) => void;
  onLines: (value: string) => void;
  onJson: (value: string) => void;
  onPicked: (value: string) => void;
  onSwitched: (value: boolean) => void;
}) {
  if (field.type === "boolean") {
    return (
      <Field label={field.label} hint={field.help}>
        <Switch checked={value === true} onChange={onSwitched} label={field.label} />
      </Field>
    );
  }
  if (field.type === "node") {
    return (
      <Field label={field.label} hint={field.help}>
        <Select
          aria-label={field.label}
          value={typeof value === "string" ? value : ""}
          onChange={onPicked}
          options={[
            ["", "Choose a node"],
            ...others.map((other): [string, string] => [other.id, other.id]),
          ]}
        />
      </Field>
    );
  }
  if (field.type === "list" || field.type === "json") {
    return (
      <Field label={field.label} hint={field.help}>
        <TextArea
          aria-label={field.label}
          value={draft}
          onChange={(event) =>
            field.type === "list" ? onLines(event.target.value) : onJson(event.target.value)
          }
          rows={4}
          mono
        />
      </Field>
    );
  }
  if (field.type === "number") {
    return (
      <Field label={field.label} hint={field.help}>
        <Input
          type="number"
          inputMode="numeric"
          min={field.min}
          aria-label={field.label}
          value={draft}
          onChange={(event) => onNumber(event.target.value)}
        />
      </Field>
    );
  }
  return (
    <Field label={field.label} hint={field.help}>
      <Input
        aria-label={field.label}
        placeholder={field.placeholder}
        value={draft}
        onChange={(event) => onText(event.target.value)}
      />
    </Field>
  );
}

/** The text each control starts with, from the parameters the node has. */
function draftsOf(params: Record<string, unknown>, fields: ParamField[]): Record<string, string> {
  const drafts: Record<string, string> = {};
  for (const field of fields) {
    const value = params[field.name];
    if (field.type === "list") {
      drafts[field.name] = Array.isArray(value) ? value.map((item) => String(item)).join("\n") : "";
    } else if (field.type === "json") {
      drafts[field.name] = value === undefined ? "" : JSON.stringify(value, null, 2);
    } else if (field.type === "text") {
      drafts[field.name] = typeof value === "string" ? value : "";
    } else if (field.type === "number") {
      drafts[field.name] = typeof value === "number" ? String(value) : "";
    }
  }
  return drafts;
}

function pick(params: Record<string, unknown>, names: string[]): Record<string, unknown> {
  return names.reduce<Record<string, unknown>>((out, name) => {
    out[name] = params[name];
    return out;
  }, {});
}

function jsonOf(value: unknown): string {
  return JSON.stringify(value, null, 2);
}
