/**
 * The properties box of the automation editor, in its two forms: those of an automation that exists, each edited where
 * it stands and saved alone, and those of one not yet saved, held by the editor until Save makes it.
 */
import { useQueryClient } from "@tanstack/react-query";
import { saveAutomation } from "../../api/calls/automations";
import { keys } from "../../api/keys";
import type { Automation } from "../../api/types/automations";
import { Input } from "../../ui/components/Input";
import { EditableProperty } from "../../ui/page/EditableProperty";
import { Field, Select } from "../../ui/page/Field";
import { Body } from "../../ui/page/PageHeader";
import { Property } from "../../ui/page/Properties";
import { Switch } from "../../ui/page/Switch";
import { type Form, formOf, nextLine, type Place, PRESETS, request, scheduleName } from "./model";

type Props = {
  places: Array<[string, string]>;
  agentNames: string[];
  now: number;
};

/** Every schedule the picker offers, and "Custom" for one it does not. */
function repeatOptions(schedule: string): Array<[string, string]> {
  const preset = PRESETS.find(([, cron]) => cron === schedule.trim());
  return [
    ...PRESETS.map(([name, cron]) => [cron, name] as [string, string]),
    ...(preset ? [] : [[schedule.trim(), "Custom"] as [string, string]]),
  ];
}

/**
 * The properties of an automation that exists, each edited where it stands and saved alone: its name, whether it runs on
 * its schedule, the schedule, its agent and where its chats work. A save sends the automation as it is stored with that
 * one change, and the list is read again once the server answered.
 */
export function SavedProperties({
  saved,
  places,
  agentNames,
  now,
  onChanged,
  onRefused,
}: Props & {
  saved: Automation;
  onChanged: (patch: Partial<Form>) => void;
  onRefused: () => void;
}) {
  const queryClient = useQueryClient();
  const save = async (patch: Partial<Form>) => {
    onRefused();
    const made = request({ ...formOf(saved), ...patch });
    if ("problem" in made) throw new Error(made.problem);
    await saveAutomation(saved.id, made.ok);
    onChanged(patch);
    await queryClient.invalidateQueries({ queryKey: keys.automations });
  };
  const stored = formOf(saved);
  const place = places.find(([value]) => value === stored.place)?.[1] ?? stored.place;
  return (
    <>
      <EditableProperty
        name="Name"
        editor={{ kind: "text", value: stored.name, onSave: (name) => save({ name }) }}
      >
        {stored.name}
      </EditableProperty>
      <EditableProperty
        name="Runs"
        editor={{
          kind: "choice",
          options: [
            ["on", "On its schedule"],
            ["off", "Only when run now"],
          ],
          value: stored.enabled ? "on" : "off",
          onSave: (value) => save({ enabled: value === "on" }),
        }}
      >
        {stored.enabled ? "On its schedule" : "Only when run now"}
      </EditableProperty>
      <EditableProperty
        name="Repeats"
        editor={{
          kind: "choice",
          options: repeatOptions(stored.schedule),
          value: stored.schedule.trim(),
          onSave: (schedule) => save({ schedule }),
        }}
      >
        {scheduleName(stored.schedule)}
      </EditableProperty>
      <EditableProperty
        name="Cron"
        editor={{ kind: "text", value: stored.schedule, onSave: (schedule) => save({ schedule }) }}
      >
        <span className="font-mono text-sm">{stored.schedule}</span>
      </EditableProperty>
      <Property name="Next">
        <span className="tabular-nums">
          {nextLine(saved, stored.schedule, stored.enabled, now)}
        </span>
      </Property>
      <EditableProperty
        name="Agent"
        editor={{
          kind: "choice",
          options: agentNames.map((name) => [name, name] as [string, string]),
          value: stored.agent,
          onSave: (agent) => save({ agent }),
        }}
      >
        {stored.agent}
      </EditableProperty>
      <EditableProperty
        name="Works in"
        editor={{
          kind: "choice",
          options: places,
          value: stored.place,
          onSave: (value) => save({ place: value as Place }),
        }}
      >
        {place}
      </EditableProperty>
    </>
  );
}

/** The properties of an automation not yet saved: the same fields, held by the editor until Save makes it. */
export function NewProperties({
  form,
  edit,
  places,
  agentNames,
  preset,
  now,
}: Props & {
  form: Form;
  edit: (change: Partial<Form>) => void;
  preset: readonly [string, string] | undefined;
}) {
  return (
    <div className="flex flex-col gap-4 px-1 py-2">
      <Switch
        checked={form.enabled}
        onChange={(next) => edit({ enabled: next })}
        label="Runs on its schedule"
      />
      <Field label="Repeats">
        {(fieldId) => (
          <Select
            id={fieldId}
            options={repeatOptions(form.schedule).map(([value, name]) =>
              !preset && name === "Custom" ? (["custom", name] as [string, string]) : [value, name],
            )}
            value={preset ? preset[1] : "custom"}
            onChange={(picked) => {
              if (picked !== "custom") edit({ schedule: picked });
            }}
          />
        )}
      </Field>
      <Field
        label="Cron, in the server's local time"
        hint="Five fields: minute, hour, day of the month, month, day of the week."
      >
        {(fieldId) => (
          <Input
            id={fieldId}
            className="font-mono text-sm"
            placeholder="0 8 * * 1-5"
            value={form.schedule}
            onChange={(event) => edit({ schedule: event.target.value })}
          />
        )}
      </Field>
      <Body>
        <span className="tabular-nums">
          {nextLine(undefined, form.schedule, form.enabled, now)}
        </span>
      </Body>
      <Field label="Agent">
        {(fieldId) => (
          <Select
            id={fieldId}
            options={agentNames}
            value={form.agent}
            onChange={(agent) => edit({ agent })}
          />
        )}
      </Field>
      <Field label="Works in">
        {(fieldId) => (
          <Select
            id={fieldId}
            options={places}
            value={form.place}
            onChange={(picked) => edit({ place: picked as Place })}
          />
        )}
      </Field>
    </div>
  );
}
