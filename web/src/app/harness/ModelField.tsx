/**
 * The Model field of the Agents and Models pages (the desktop app's `model_picker`): a list of the router's model
 * classes and the models the upstreams serve, whose last entry, "Another model…", turns the field into an input, so
 * that a model no upstream lists can be entered. Without a list the field is an input. `ModelNote` says which upstream
 * could not be asked and offers to ask them again.
 */
import { useState } from "react";
import type { AvailableModels } from "../../api/types/chat";
import { choiceLabel, type ModelChoice } from "../agents/model";
import { Select, TextInput } from "./form";

const OTHER = "\u0000other";

export function ModelField({
  id,
  choices,
  value,
  placeholder,
  onChange,
  onPick,
}: {
  id: string;
  choices: ModelChoice[];
  value: string;
  placeholder: string;
  onChange: (model: string) => void;
  /** A model was picked from the list. */
  onPick: (choice: ModelChoice) => void;
}) {
  const [typing, setTyping] = useState(false);
  if (typing || choices.length === 0) {
    return (
      <TextInput
        id={id}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
    );
  }
  const options: Array<[string, string]> = choices.map((choice) => [
    choice.id,
    choiceLabel(choice),
  ]);
  if (value !== "" && !choices.some((choice) => choice.id === value))
    options.unshift([value, value]);
  return (
    <Select
      id={id}
      value={value}
      options={[["", placeholder], ...options, [OTHER, "Another model…"]]}
      onChange={(picked) => {
        if (picked === OTHER) setTyping(true);
        else {
          const choice = choices.find((each) => each.id === picked);
          if (choice) onPick(choice);
          else onChange(picked);
        }
      }}
    />
  );
}

/** What the upstreams said that went wrong, and the link that asks them again. */
export function ModelNote({
  available,
  onRefresh,
  refreshing,
}: {
  available: AvailableModels | undefined;
  onRefresh: () => void;
  refreshing?: boolean;
}) {
  if (!available) return null;
  return (
    <div className="flex items-start justify-between gap-base font-mono text-[11px] text-low">
      <div className="flex flex-col gap-px">
        {available.errors.map((error) => (
          <span key={error.upstream}>
            Could not ask {error.upstream}: {error.message}
          </span>
        ))}
      </div>
      <button
        type="button"
        disabled={refreshing}
        onClick={onRefresh}
        className="shrink-0 text-normal hover:text-high disabled:opacity-60"
      >
        {refreshing ? "Asking…" : "Refresh"}
      </button>
    </div>
  );
}
