/** The names of the secrets the server holds, for the fields that name one rather than carry its value. */

import { useQuery } from "@tanstack/react-query";
import { settings } from "../../../api/calls/settings";
import { keys } from "../../../api/keys";
import { Field, Select } from "../../../ui/page/Field";

/** The names of the secrets in `GET /v1/settings`, never the values. */
function useSecretNames(): string[] {
  const query = useQuery({ queryKey: keys.settings, queryFn: settings });
  return query.data?.secrets ?? [];
}

/**
 * A field that names a secret. The choices are the names the server holds, so a value is never typed into, or read
 * out of, the form; a name the file already holds stays among them even when the server no longer has it.
 */
export function SecretField({
  label,
  value,
  onChange,
  hint,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  hint?: string;
}) {
  const names = useSecretNames();
  const options: Array<[string, string]> = [["", "No secret"]];
  for (const name of names) options.push([name, name]);
  if (value !== "" && !names.includes(value)) options.push([value, value]);
  return (
    <Field label={label} hint={hint}>
      <Select
        aria-label={label}
        options={options}
        value={value}
        onChange={(next) => onChange(next)}
      />
    </Field>
  );
}
