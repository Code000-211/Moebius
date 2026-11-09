/**
 * The forms of a workflow's trigger: the values a person gives a manual run, the path and secret a webhook is called at,
 * and the cron a schedule fires on. A cron the server would not parse is shown as a problem, and the canvas will not
 * save it.
 */

import { useState } from "react";
import type { Trigger } from "../../../api/types/workflows";
import { Button } from "../../../ui/components/Button";
import { Input } from "../../../ui/components/Input";
import { Field, Select } from "../../../ui/page/Field";
import { Switch } from "../../../ui/page/Switch";
import type { TriggerFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";
import { SecretField } from "./secrets";

const INPUT_TYPES = ["text", "number", "boolean", "choice", "repository"];

function paramsOf(trigger: Trigger): Record<string, unknown> {
  return (trigger.params ?? {}) as Record<string, unknown>;
}

function give(
  trigger: Trigger,
  onTrigger: (trigger: Trigger) => void,
  next: Record<string, unknown>,
) {
  onTrigger({ ...trigger, params: next });
}

function withKey(
  params: Record<string, unknown>,
  name: string,
  value: unknown,
): Record<string, unknown> {
  const next = { ...params };
  if (value === undefined || value === "") {
    delete next[name];
  } else {
    next[name] = value;
  }
  return next;
}

/** A row of the manual trigger's inputs as the form edits it. */
interface InputRow {
  key: string;
  name: string;
  type: string;
  required: boolean;
  value: string;
  choices: string;
}

let rowKeys = 0;

function nextRowKey(): string {
  rowKeys += 1;
  return `input-${rowKeys}`;
}

function inputRows(trigger: Trigger): InputRow[] {
  const inputs = paramsOf(trigger).inputs;
  if (!Array.isArray(inputs)) return [];
  return inputs.flatMap((each): InputRow[] => {
    if (each === null || typeof each !== "object") return [];
    const table = each as Record<string, unknown>;
    const type = typeof table.type === "string" ? table.type : "text";
    const choices = Array.isArray(table.choices)
      ? table.choices.filter((choice): choice is string => typeof choice === "string").join(", ")
      : "";
    const value =
      table.default === undefined || table.default === null ? "" : String(table.default);
    return [
      {
        key: nextRowKey(),
        name: typeof table.name === "string" ? table.name : "",
        type,
        required: table.required === true,
        value,
        choices,
      },
    ];
  });
}

function inputParams(rows: InputRow[]): Array<Record<string, unknown>> {
  return rows
    .filter((row) => row.name.trim() !== "")
    .map((row) => {
      const table: Record<string, unknown> = { name: row.name, type: row.type };
      if (row.required) table.required = true;
      if (row.type === "choice") {
        const choices = row.choices
          .split(",")
          .map((choice) => choice.trim())
          .filter((choice) => choice !== "");
        if (choices.length > 0) table.choices = choices;
      }
      if (row.value !== "") {
        if (row.type === "number" && Number.isFinite(Number(row.value))) {
          table.default = Number(row.value);
        } else if (row.type === "boolean") {
          table.default = row.value === "true";
        } else if (row.type !== "number") {
          table.default = row.value;
        }
      }
      return table;
    });
}

/** The inputs of a manual trigger, in the order a person is shown them. */
function ManualInputs({
  trigger,
  onTrigger,
}: TriggerFormProps & { trigger: Trigger; onTrigger: (trigger: Trigger) => void }) {
  const [rows, setRows] = useState<InputRow[]>(() => inputRows(trigger));
  const change = (next: InputRow[]) => {
    setRows(next);
    give(trigger, onTrigger, withKey(paramsOf(trigger), "inputs", inputParams(next)));
  };

  return (
    <div className="flex flex-col gap-3">
      <span className="text-sm font-medium text-primary">Inputs</span>
      {rows.map((row, index) => (
        <div key={row.key} className="flex flex-col gap-2 rounded-md border border-hairline p-3">
          <div className="flex items-end gap-2">
            <Field label={`Input ${index + 1} name`}>
              <Input
                aria-label={`Input ${index + 1} name`}
                value={row.name}
                onChange={(event) => {
                  const next = [...rows];
                  next[index] = { ...row, name: event.target.value };
                  change(next);
                }}
              />
            </Field>
            <Field label={`Input ${index + 1} type`}>
              <Select
                aria-label={`Input ${index + 1} type`}
                value={row.type}
                options={INPUT_TYPES.map((type): [string, string] => [type, type])}
                onChange={(type) => {
                  const next = [...rows];
                  next[index] = { ...row, type, choices: type === "choice" ? row.choices : "" };
                  change(next);
                }}
              />
            </Field>
            <Button
              variant="secondary"
              onClick={() => change(rows.filter((_, at) => at !== index))}
            >
              Remove input
            </Button>
          </div>
          <Field label="Required" hint="Whether a run must give this input a value.">
            <Switch
              checked={row.required}
              label={`Input ${index + 1} required`}
              onChange={(required) => {
                const next = [...rows];
                next[index] = { ...row, required };
                change(next);
              }}
            />
          </Field>
          {row.type === "choice" ? (
            <Field
              label={`Input ${index + 1} choices`}
              hint="The values it may take, separated by commas."
            >
              <Input
                aria-label={`Input ${index + 1} choices`}
                value={row.choices}
                placeholder="staging, prod"
                onChange={(event) => {
                  const next = [...rows];
                  next[index] = { ...row, choices: event.target.value };
                  change(next);
                }}
              />
            </Field>
          ) : null}
          <Field
            label={`Input ${index + 1} default`}
            hint="What the input takes when a run gives it none."
          >
            {row.type === "boolean" ? (
              <Select
                aria-label={`Input ${index + 1} default`}
                value={row.value === "" ? "" : row.value}
                options={[
                  ["", "None"],
                  ["true", "true"],
                  ["false", "false"],
                ]}
                onChange={(value) => {
                  const next = [...rows];
                  next[index] = { ...row, value };
                  change(next);
                }}
              />
            ) : (
              <Input
                type={row.type === "number" ? "number" : "text"}
                aria-label={`Input ${index + 1} default`}
                value={row.value}
                onChange={(event) => {
                  const next = [...rows];
                  next[index] = { ...row, value: event.target.value };
                  change(next);
                }}
              />
            )}
          </Field>
        </div>
      ))}
      <Button
        variant="secondary"
        onClick={() =>
          change([
            ...rows,
            { key: nextRowKey(), name: "", type: "text", required: false, value: "", choices: "" },
          ])
        }
      >
        Add input
      </Button>
    </div>
  );
}

/** The manual trigger: the inputs a person fills in before a run starts. */
export function ManualTriggerForm({ trigger, onTrigger }: TriggerFormProps) {
  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="trigger-form">
      <ManualInputs trigger={trigger} onTrigger={onTrigger} />
      <ParamsPreview params={paramsOf(trigger)} />
    </div>
  );
}

/** The webhook trigger: the path it is called at, the secret that call must carry, and its repository. */
export function WebhookTriggerForm({ trigger, onTrigger }: TriggerFormProps) {
  const params = paramsOf(trigger);
  const path = typeof params.path === "string" ? params.path : "";
  const secret = typeof params.secret === "string" ? params.secret : "";
  const repository = typeof params.repository === "string" ? params.repository : "";
  const update = (name: string, value: unknown) =>
    give(trigger, onTrigger, withKey(params, name, value));

  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="trigger-form">
      <Field
        label="Path"
        hint="The path a call to /api/hooks/ starts this workflow at."
        error={path.trim() === "" ? "A webhook trigger needs its path." : undefined}
      >
        <Input
          aria-label="Path"
          mono
          value={path}
          placeholder="release"
          onChange={(event) => update("path", event.target.value)}
        />
      </Field>
      <SecretField
        label="Secret"
        value={secret}
        onChange={(value) => update("secret", value)}
        hint="The name of the secret the call is checked against; its value is never written here."
      />
      <Field label="Repository" hint="The repository a run of this workflow belongs to.">
        <Input
          aria-label="Repository"
          value={repository}
          placeholder="moebius"
          onChange={(event) => update("repository", event.target.value)}
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}

/** The number of each field of a five-field cron, and the values it allows. */
const CRON_FIELDS: Array<{ name: string; min: number; max: number; names: string[] }> = [
  { name: "minute", min: 0, max: 59, names: [] },
  { name: "hour", min: 0, max: 23, names: [] },
  { name: "day of the month", min: 1, max: 31, names: [] },
  {
    name: "month",
    min: 1,
    max: 12,
    names: ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"],
  },
  // Sunday is 0 or 7, as the server's own parser has it.
  {
    name: "day of the week",
    min: 0,
    max: 7,
    names: ["sun", "mon", "tue", "wed", "thu", "fri", "sat"],
  },
];

function cronValue(field: (typeof CRON_FIELDS)[number], text: string): number | null {
  const lowered = text.trim().toLowerCase();
  if (lowered === "") return null;
  const named = field.names.indexOf(lowered);
  if (named !== -1) return named + (field.name === "month" ? 1 : 0);
  const value = Number(lowered);
  return Number.isInteger(value) ? value : null;
}

function fieldProblem(field: (typeof CRON_FIELDS)[number], text: string): string | null {
  const refused = (what: string) => `the ${field.name} ${what} is a number`;
  for (const item of text.split(",")) {
    if (item.trim() === "") return `the ${field.name} "${item}" is a number`;
    const [range = "", stepText, ...extra] = item.split("/");
    if (extra.length > 0) return `the ${field.name} "${item}" has one step`;
    let step = 1;
    if (stepText !== undefined) {
      step = Number(stepText);
      if (!Number.isInteger(step) || step < 1) {
        return `the step of the ${field.name} "${item}" is a number at least one`;
      }
    }
    let from: number | null;
    let to: number | null;
    if (range.trim() === "*") {
      from = field.min;
      to = field.max;
    } else if (range.includes("-")) {
      const [left = "", right = "", ...rest] = range.split("-");
      if (rest.length > 0) return `the ${field.name} "${item}" has one range`;
      from = cronValue(field, left);
      to = cronValue(field, right);
    } else {
      from = cronValue(field, range);
      if (from === null) return refused(`"${range}"`);
      if (from < field.min || from > field.max) {
        return `the ${field.name} "${range}" is not between ${field.min} and ${field.max}`;
      }
      to = stepText === undefined ? from : field.max;
    }
    if (from === null || to === null) return refused(`"${item}"`);
    if (from < field.min || from > field.max) {
      return `the ${field.name} "${from}" is not between ${field.min} and ${field.max}`;
    }
    if (to < field.min || to > field.max) {
      return `the ${field.name} "${to}" is not between ${field.min} and ${field.max}`;
    }
    if (from > to) return `the ${field.name} "${item}" starts after it ends`;
  }
  return null;
}

/** Why a cron would not parse, or `null` when it would. */
function cronProblem(cron: string): string | null {
  const fields = cron
    .trim()
    .split(/\s+/)
    .filter((part) => part !== "");
  if (fields.length !== 5) return "a cron has five fields: minute, hour, day, month and weekday";
  for (let index = 0; index < CRON_FIELDS.length; index += 1) {
    const field = CRON_FIELDS[index];
    const text = fields[index];
    if (field === undefined || text === undefined) continue;
    const problem = fieldProblem(field, text);
    if (problem !== null) return problem;
  }
  return null;
}

/** Why a trigger's parameters would not parse, or `null` when they would: only a schedule's cron is checked here. */
export function triggerProblem(trigger: Trigger | undefined): string | null {
  if (trigger?.kind !== "schedule") return null;
  const cron = paramsOf(trigger).cron;
  return cronProblem(typeof cron === "string" ? cron : "");
}

/** The schedule trigger: the cron it fires on and the repository a run belongs to. */
export function ScheduleTriggerForm({ trigger, onTrigger }: TriggerFormProps) {
  const params = paramsOf(trigger);
  const cron = typeof params.cron === "string" ? params.cron : "";
  const repository = typeof params.repository === "string" ? params.repository : "";
  const problem = cronProblem(cron);
  const update = (name: string, value: unknown) =>
    give(trigger, onTrigger, withKey(params, name, value));

  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="trigger-form">
      <Field
        label="Cron"
        hint="Five fields: minute, hour, day of the month, month and day of the week, in the server's local time."
        error={problem ?? undefined}
      >
        <Input
          aria-label="Cron"
          mono
          value={cron}
          placeholder="0 6 * * 1"
          onChange={(event) => update("cron", event.target.value)}
        />
      </Field>
      <Field label="Repository" hint="The repository a run of this workflow belongs to.">
        <Input
          aria-label="Repository"
          value={repository}
          placeholder="moebius"
          onChange={(event) => update("repository", event.target.value)}
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}
