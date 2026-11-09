/** The form of a `condition` node: the ordered ports and their expressions, and the optional fallback. */
import { useRef, useState } from "react";
import { Button } from "../../../ui/components/Button";
import { Input } from "../../../ui/components/Input";
import { Field } from "../../../ui/page/Field";
import type { NodeFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";

type Port = { name: string; when: string };
type Row = Port & { key: number };

const blank: Port = { name: "", when: "" };

function readPorts(params: Record<string, unknown>): Port[] {
  if (!Array.isArray(params.ports)) {
    return [];
  }
  return params.ports.flatMap((entry): Port[] => {
    if (typeof entry !== "object" || entry === null) {
      return [];
    }
    const port = entry as Record<string, unknown>;
    return [
      {
        name: typeof port.name === "string" ? port.name : "",
        when: typeof port.when === "string" ? port.when : "",
      },
    ];
  });
}

function readFallback(params: Record<string, unknown>): string {
  return typeof params.fallback === "string" ? params.fallback : "";
}

export function ConditionForm({ node, onParams }: NodeFormProps) {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const nextKey = useRef(0);
  const [ports, setPorts] = useState<Row[]>(() => {
    const rows = readPorts(params);
    return (rows.length > 0 ? rows : [blank]).map((port) => ({
      ...port,
      key: nextKey.current++,
    }));
  });
  const [fallback, setFallback] = useState(() => readFallback(params));

  const blankPort = ports.some((port) => port.name.trim() === "" || port.when.trim() === "");
  const duplicate =
    new Set(ports.map((port) => port.name).filter((name) => name !== "")).size !==
    ports.filter((port) => port.name !== "").length;
  const problem = blankPort
    ? "Every condition port needs a name and an expression."
    : duplicate
      ? "Two ports cannot have the same name."
      : undefined;

  function give(next: Port[], nextFallback = fallback) {
    const changed = { ...params };
    const stored = next
      .filter((port) => port.name !== "" || port.when !== "")
      .map((port) => ({ name: port.name, when: port.when }));
    if (stored.length > 0 || "ports" in params) {
      changed.ports = stored;
    }
    if (nextFallback.trim() === "") {
      delete changed.fallback;
    } else {
      changed.fallback = nextFallback;
    }
    onParams(changed);
  }

  function change(index: number, part: keyof Port, value: string) {
    const next = ports.map((port, at) => (at === index ? { ...port, [part]: value } : port));
    setPorts(next);
    give(next);
  }

  function add() {
    const next = [...ports, { ...blank, key: nextKey.current++ }];
    setPorts(next);
    give(next);
  }

  function remove(index: number) {
    const remaining = ports.filter((_, at) => at !== index);
    const next = remaining.length > 0 ? remaining : [{ ...blank, key: nextKey.current++ }];
    setPorts(next);
    give(remaining);
  }

  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="condition-form">
      <Field
        label="Ports"
        hint="Tried in order: the value leaves by the first port whose expression is true. Expressions such as input.ok are Luau."
        error={problem}
      >
        <div className="flex flex-col gap-2">
          {ports.map((port, index) => (
            <div key={port.key} className="flex items-end gap-2">
              <Input
                aria-label={`Port ${index + 1} name`}
                value={port.name}
                onChange={(event) => change(index, "name", event.target.value)}
                placeholder="pass"
              />
              <Input
                aria-label={`Port ${index + 1} when`}
                value={port.when}
                onChange={(event) => change(index, "when", event.target.value)}
                placeholder="input.ok"
                mono
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label={`Remove port ${index + 1}`}
                onClick={() => remove(index)}
              >
                ×
              </Button>
            </div>
          ))}
          <div>
            <Button type="button" variant="secondary" size="sm" onClick={add}>
              Add port
            </Button>
          </div>
        </div>
      </Field>
      <Field
        label="Fallback"
        hint="The port taken when no expression is true. Left empty, the node fails closed instead."
      >
        <Input
          aria-label="Fallback"
          value={fallback}
          onChange={(event) => {
            setFallback(event.target.value);
            give(ports, event.target.value);
          }}
          placeholder="other"
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}
