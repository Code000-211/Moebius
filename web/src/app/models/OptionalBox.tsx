/**
 * The teacher or the decision model on the Models page: a box that can be turned on and off, with the fields
 * `moebius.toml` has for it and, for the decision model, the field of its key.
 */
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { FieldRow, Rows, Select } from "../../ui/page/Field";
import type { modelChoices } from "../agents/model";
import { ModelField } from "../harness/ModelField";
import { Section } from "../settings/parts";
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
      <Section title={heading}>
        <Rows>
          <FieldRow
            label={heading}
            description={optional.wasOn ? "Turned off; Save takes it out of the file." : off}
          >
            <Button
              variant="secondary"
              size="sm"
              aria-label={`Turn on the ${heading.toLowerCase()}`}
              onClick={onToggle}
            >
              Turn on
            </Button>
          </FieldRow>
        </Rows>
      </Section>
    );
  }
  return (
    <Section title={heading}>
      <p className="m-0 mb-3 max-w-prose text-sm text-secondary">{about}</p>
      <Rows>
        {optional.fields.map((field: FieldForm) => (
          <FieldRow
            key={field.key}
            label={field.label}
            description={
              field.default !== null && field.text.trim() === ""
                ? `Default ${field.default}`
                : undefined
            }
            wide
          >
            {field.kind === "model" ? (
              <ModelField
                choices={choices}
                value={field.text}
                placeholder={field.default ?? ""}
                onChange={(text) => onField(field.key, text)}
                onPick={onPick}
              />
            ) : field.kind === "pick" ? (
              <Select
                options={[...PROTOCOLS]}
                value={field.text}
                onChange={(text) => onField(field.key, text)}
              />
            ) : (
              <Input
                mono={field.kind === "code" || field.kind === "list"}
                className={
                  field.kind === "whole" || field.kind === "decimal" ? "tabular-nums" : undefined
                }
                value={field.text}
                placeholder={field.default ?? ""}
                onChange={(event) => onField(field.key, event.target.value)}
              />
            )}
          </FieldRow>
        ))}
        {optional.key && onKey && (
          <KeyField
            label="Key of the decision model"
            choice={optional.key.now}
            secrets={secrets ?? []}
            onChange={onKey}
          />
        )}
        <FieldRow label={`Turn off the ${heading.toLowerCase()}`}>
          <Button
            variant="secondary"
            size="sm"
            aria-label={`Turn off the ${heading.toLowerCase()}`}
            onClick={onToggle}
          >
            Turn off
          </Button>
        </FieldRow>
      </Rows>
    </Section>
  );
}
