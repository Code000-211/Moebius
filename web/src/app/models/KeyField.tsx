/**
 * Where an API key comes from, as rows of a settings area: no key, an environment variable, or a key typed here and
 * sent once. The rows carry no box of their own; they sit among the rows of the provider they belong to.
 */
import { Input } from "../../ui/components/Input";
import { FieldRow, Select } from "../../ui/page/Field";
import { DeleteKey } from "./DeleteKey";
import { KEY_MODES, type KeyChoice, type KeyMode } from "./model";

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
    <>
      <FieldRow label="Key" description={keyHint(choice, stored)} wide>
        <Select
          aria-label={`${label}: where it comes from`}
          options={KEY_MODES}
          value={choice.mode}
          onChange={(mode) => onChange({ mode: mode as KeyMode })}
        />
      </FieldRow>
      {choice.mode === "env" && (
        <FieldRow
          label="Environment variable"
          description="The server reads it from its own environment when a run starts."
          wide
        >
          <Input
            mono
            aria-label={`${label}: environment variable`}
            value={choice.env}
            placeholder="ANTHROPIC_API_KEY"
            onChange={(event) => onChange({ env: event.target.value })}
          />
        </FieldRow>
      )}
      {choice.mode === "typed" && (
        <FieldRow
          label="API key"
          description={
            stored
              ? "Stored on the server by name. Type to replace it; it is never shown back."
              : "Sent once, then stored on the server by name and never shown back."
          }
          wide
        >
          <Input
            type="password"
            autoComplete="new-password"
            aria-label={`${label}: the key`}
            value={choice.typed}
            placeholder={stored ? "Stored; type to replace it" : "Paste the key"}
            onChange={(event) => onChange({ typed: event.target.value })}
          />
        </FieldRow>
      )}
      {choice.mode === "typed" && stored && choice.stored !== null && (
        <FieldRow label="Stored key" description={choice.stored}>
          <DeleteKey name={choice.stored} />
        </FieldRow>
      )}
    </>
  );
}

/** Whether a key is there, in a word, under the choice of where it comes from. */
function keyHint(choice: KeyChoice, stored: boolean): string {
  if (choice.mode === "env")
    return choice.env.trim() === "" ? "Name the variable the server reads." : "Configured.";
  if (choice.mode === "typed") return stored ? "Configured." : "Needs a key.";
  return "No key: the address is open, or a login below supplies it.";
}
