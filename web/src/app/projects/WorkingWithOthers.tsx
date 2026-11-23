/**
 * The "Working with others" box of a repository on a forge (ADR 0027): what this server does so that people who each run
 * a server on the same repository do not do the same issue twice, through the forge alone. It marks the issues that are
 * ready with a label (only the owner's server should), and it says which issues this server's agents take: every approved
 * one, or only those that carry the label and nobody took. Saved with one button.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { editWorkingWithOthers } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { AgentsTake, Repository } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Switch } from "../../ui/page/Switch";
import { forgeName } from "../board/model";

/** The label until a person gives another. */
export const DEFAULT_READY_LABEL = "Ready for agent";

export function WorkingWithOthers({ repository }: { repository: Repository }) {
  const queryClient = useQueryClient();
  const forge = forgeName(repository) ?? "the forge";
  const stored = {
    mark: repository.mark_ready ?? false,
    label: repository.ready_label ?? DEFAULT_READY_LABEL,
    takes: repository.agents_take ?? "all",
  };
  const [mark, setMark] = useState(stored.mark);
  const [label, setLabel] = useState(stored.label);
  const [takes, setTakes] = useState<AgentsTake>(stored.takes);
  const trimmed = label.trim();
  const changed = mark !== stored.mark || trimmed !== stored.label || takes !== stored.takes;
  const save = useMutation({
    mutationFn: () =>
      editWorkingWithOthers(repository.id, {
        mark_ready: mark,
        ready_label: trimmed,
        agents_take: takes,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
    },
  });
  return (
    <Box title="Working with others" testId="working-with-others" strong>
      <form
        className="flex max-w-xl flex-col gap-5"
        onSubmit={(e) => {
          e.preventDefault();
          if (changed && trimmed !== "" && !save.isPending) save.mutate();
        }}
      >
        <p className="m-0 text-base text-secondary">
          {`People who each run a server on this repository coordinate through ${forge} alone: taking an issue makes you its assignee there, and an issue someone else is assigned to is left alone.`}
        </p>
        <div className="flex items-start justify-between gap-4">
          <div className="flex min-w-0 flex-col gap-0.5">
            <span id="mark-ready" className="text-base font-medium text-text">
              {`Mark ready issues on ${forge}`}
            </span>
            <span className="text-sm text-muted">
              Adds the label to every open issue that is approved and has no open blocker, and takes
              it off as soon as one appears. Only the server of the person who owns the repository
              should turn this on.
            </span>
          </div>
          <Switch label={`Mark ready issues on ${forge}`} checked={mark} onChange={setMark} />
        </div>
        <Field
          label="Label"
          hint="The name of the label, made on the forge when it does not exist."
        >
          <Input
            value={label}
            autoComplete="off"
            placeholder={DEFAULT_READY_LABEL}
            onChange={(e) => setLabel(e.target.value)}
          />
        </Field>
        <Field
          label="Agents take"
          hint="Helpers set the second, so that their servers start only the issues the owner's server released."
        >
          <Select
            value={takes}
            onChange={(next) => setTakes(next as AgentsTake)}
            options={[
              ["all", "All approved issues"],
              [
                "labelled",
                `Only issues labelled "${trimmed || DEFAULT_READY_LABEL}" and not assigned`,
              ],
            ]}
          />
        </Field>
        <div className="flex gap-2">
          <Button type="submit" size="sm" disabled={!changed || trimmed === "" || save.isPending}>
            {save.isPending ? "Saving" : "Save settings"}
          </Button>
        </div>
        {save.isError && (
          <Notice tone="error">
            <span className="font-semibold">Couldn't save this. It stays as it was. </span>
            {isApiError(save.error) ? save.error.message : "The settings could not be saved."}
          </Notice>
        )}
      </form>
    </Box>
  );
}
