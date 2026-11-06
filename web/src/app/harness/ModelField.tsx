/**
 * The Model field of the Agents and Models pages (the former desktop app's `model_picker`): a list of the router's model
 * classes and the models the upstreams serve, whose last entry, "Another model", turns the field into an input, so
 * that a model no upstream lists can be entered. Without a list the field is an input. `ModelNote` says which upstream
 * could not be asked, what to do about it, and offers to ask them again; `Answered` marks that the last ask came back
 * with nothing wrong.
 */
import { CheckIcon } from "@phosphor-icons/react";
import { useState } from "react";
import type { AvailableModels } from "../../api/types/chat";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { cn } from "../../ui/lib/cn";
import { plural } from "../../ui/lib/format";
import { Select } from "../../ui/page/Field";
import { choiceLabel, type ModelChoice } from "../agents/model";

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
      <Input
        id={id}
        mono
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
      options={[["", placeholder], ...options, [OTHER, "Another model"]]}
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

/** The sentence for an upstream that did not answer, kept in one element so it can be quoted whole. */
export function couldNotAsk(upstream: string, message: string): string {
  return `Could not ask ${upstream}: ${message}`;
}

/** What to do about an upstream that did not answer, beside the sentence that says it failed. */
function HowToFixCouldNotAsk() {
  return (
    <span className="text-sm text-secondary">
      Check its address and its key, then ask it again.
    </span>
  );
}

/**
 * That the upstreams answered when last they were asked. Asking them usually works, so this is a quiet check rather
 * than a pill: the rare answer that went wrong is said in words, in red, above it.
 */
export function Answered({ className }: { className?: string }) {
  return (
    <span
      role="img"
      aria-label="Answered"
      className={cn("inline-flex shrink-0 items-center text-secondary", className)}
    >
      <CheckIcon aria-hidden="true" className="size-4" />
    </span>
  );
}

/** What the upstreams said that went wrong, and the control that asks them again. */
export function ModelNote({
  available,
  onRefresh,
  refreshing,
  answered,
}: {
  available: AvailableModels | undefined;
  onRefresh: () => void;
  refreshing?: boolean;
  /** The last ask answered with nothing wrong. */
  answered?: boolean;
}) {
  if (!available) return null;
  const upstreams = new Set(available.models.map((model) => model.upstream)).size;
  return (
    <div className="flex flex-col gap-3">
      {available.errors.map((error) => (
        <div key={error.upstream} className="flex flex-col gap-1">
          <span className="break-words text-sm text-error-text">
            {couldNotAsk(error.upstream, error.message)}
          </span>
          <HowToFixCouldNotAsk />
        </div>
      ))}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <span className="min-w-0 break-words text-sm tabular-nums text-secondary">
          {available.errors.length > 0 && "The rest answered with "}
          {plural(available.models.length, "model", "models")} from{" "}
          {plural(upstreams, "provider", "providers")}
        </span>
        <span className="flex items-center gap-2">
          {answered && <Answered />}
          <Button variant="secondary" size="sm" disabled={refreshing} onClick={onRefresh}>
            {refreshing ? "Asking" : "Refresh"}
          </Button>
        </span>
      </div>
    </div>
  );
}
