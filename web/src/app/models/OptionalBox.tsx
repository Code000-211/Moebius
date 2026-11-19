/**
 * The teacher or the decision model on the Models page: a box that can be turned on and off, with the fields
 * `moebius.toml` has for it and, for the decision model, the field of its key.
 */
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field, Select } from "../../ui/page/Field";
import type { modelChoices } from "../agents/model";
import { ModelField } from "../harness/ModelField";
import { KeyField } from "./KeyField";
import { type Field as FieldForm, type KeyChoice, type OptionalForm, PROTOCOLS } from "./model";

/** The teacher or the decision model: a box that can be turned on and off. */
export function OptionalBox({
  heading,
  about,
  off,
  optional,
  choices,
  secrets,
  onToggle,
  onField,
  onPick,
  onKey,
}: {
  heading: string;
  about: string;
  off: string;
  optional: OptionalForm;
  choices: ReturnType<typeof modelChoices>;
  secrets?: string[];
  onToggle: () => void;
  onField: (key: string, text: string) => void;
  onPick: (choice: ReturnType<typeof modelChoices>[number]) => void;
  onKey?: (change: Partial<KeyChoice>) => void;
}) {
  if (!optional.on) {
    return (
      <Box title={heading} className="flex flex-col gap-4">
        <p className="m-0 max-w-prose text-base text-secondary">
          {optional.wasOn ? "Turned off; Save takes it out of the file." : off}
        </p>
        <div>
          <Button
            variant="secondary"
            size="sm"
            aria-label={`Turn on the ${heading.toLowerCase()}`}
            onClick={onToggle}
          >
            Turn on
          </Button>
        </div>
      </Box>
    );
  }
  return (
    <Box title={heading} className="flex flex-col gap-4">
      <p className="m-0 max-w-prose text-base text-secondary">{about}</p>
      <div className="grid items-start gap-4 sm:grid-cols-2">
        {optional.fields.map((field: FieldForm) => (
          <Field
            key={field.key}
            label={field.label}
            hint={
              field.default !== null && field.text.trim() === ""
                ? `Default ${field.default}`
                : undefined
            }
          >
            {(id) =>
              field.kind === "model" ? (
                <ModelField
                  id={id}
                  choices={choices}
                  value={field.text}
                  placeholder={field.default ?? ""}
                  onChange={(text) => onField(field.key, text)}
                  onPick={onPick}
                />
              ) : field.kind === "pick" ? (
                <Select
                  id={id}
                  options={[...PROTOCOLS]}
                  value={field.text}
                  onChange={(text) => onField(field.key, text)}
                />
              ) : (
                <Input
                  id={id}
                  mono={field.kind === "code" || field.kind === "list"}
                  className={
                    field.kind === "whole" || field.kind === "decimal" ? "tabular-nums" : undefined
                  }
                  value={field.text}
                  placeholder={field.default ?? ""}
                  onChange={(event) => onField(field.key, event.target.value)}
                />
              )
            }
          </Field>
        ))}
      </div>
      {optional.key && onKey && (
        <KeyField
          label="Key of the decision model"
          choice={optional.key.now}
          secrets={secrets ?? []}
          onChange={onKey}
        />
      )}
      <div>
        <Button
          variant="secondary"
          size="sm"
          aria-label={`Turn off the ${heading.toLowerCase()}`}
          onClick={onToggle}
        >
          Turn off
        </Button>
      </div>
    </Box>
  );
}
