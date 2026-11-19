import { Input } from "../../ui/components/Input";
import { Field, Select } from "../../ui/page/Field";
import { DeleteKey } from "./DeleteKey";
import { KEY_MODES, type KeyChoice, type KeyMode } from "./model";

/** Where an API key comes from: no key, an environment variable, or a key typed here and sent once. */
export function KeyField({
  label,
  choice,
  secrets,
  onChange,
}: {
  label: string;
  choice: KeyChoice;
  secrets: string[];
  onChange: (change: Partial<KeyChoice>) => void;
}) {
  const stored = choice.stored !== null && secrets.includes(choice.stored);
  return (
    <div className="flex flex-col gap-4">
      <div className="grid items-start gap-4 sm:grid-cols-2">
        <Field label="Key" hint={keyHint(choice, stored)}>
          {(id) => (
            <Select
              id={id}
              aria-label={`${label}: where it comes from`}
              options={KEY_MODES}
              value={choice.mode}
              onChange={(mode) => onChange({ mode: mode as KeyMode })}
            />
          )}
        </Field>
        {choice.mode === "env" && (
          <Field
            label="Environment variable"
            hint="The server reads it from its own environment when a run starts."
          >
            {(id) => (
              <Input
                id={id}
                mono
                aria-label={`${label}: environment variable`}
                value={choice.env}
                placeholder="ANTHROPIC_API_KEY"
                onChange={(event) => onChange({ env: event.target.value })}
              />
            )}
          </Field>
        )}
        {choice.mode === "typed" && (
          <Field
            label="API key"
            hint={
              stored
                ? "Stored on the server by name. Type to replace it; it is never shown back."
                : "Sent once, then stored on the server by name and never shown back."
            }
          >
            {(id) => (
              <Input
                id={id}
                type="password"
                autoComplete="new-password"
                aria-label={`${label}: the key`}
                value={choice.typed}
                placeholder={stored ? "Stored; type to replace it" : "Paste the key"}
                onChange={(event) => onChange({ typed: event.target.value })}
              />
            )}
          </Field>
        )}
        {choice.mode === "typed" && stored && choice.stored !== null && (
          <DeleteKey name={choice.stored} />
        )}
      </div>
      {choice.mode === "none" && (
        <p className="m-0 text-sm text-secondary">
          No key: the address is open, or a login below supplies it.
        </p>
      )}
    </div>
  );
}

/** Whether a key is there, in a word, under the choice of where it comes from. */
function keyHint(choice: KeyChoice, stored: boolean): string {
  if (choice.mode === "env")
    return choice.env.trim() === "" ? "Name the variable the server reads." : "Configured.";
  if (choice.mode === "typed") return stored ? "Configured." : "Needs a key.";
  return "No key.";
}
