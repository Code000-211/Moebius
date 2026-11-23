/**
 * The "Working with others" box of a repository on a forge (ADR 0027): what this server does so that people who each run
 * a server on the same repository do not do the same issue twice, through the forge alone. It marks the issues that are
 * ready with a label (only the owner's server should), and it says which issues this server's agents take: every approved
 * one, or only those that carry the label and nobody took. A second part, "Commands in issue comments", lets people who
 * may comment on an issue but not assign it take, release and finish issues with `/take`, `/release` and `/done`, which the
 * server does for them with its own token. Saved with one button.
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

/** How many issues a login may hold that it took with `/take`, until a person sets another number. */
export const DEFAULT_COMMAND_LIMIT = 3;
const MAX_COMMAND_LIMIT = 50;

export function WorkingWithOthers({ repository }: { repository: Repository }) {
  const queryClient = useQueryClient();
  const forge = forgeName(repository) ?? "the forge";
  const stored = {
    mark: repository.mark_ready ?? false,
    label: repository.ready_label ?? DEFAULT_READY_LABEL,
    takes: repository.agents_take ?? "all",
    commands: repository.commands ?? false,
    limit: repository.command_limit ?? DEFAULT_COMMAND_LIMIT,
    answer: repository.answer_on_forge ?? true,
  };
  const [mark, setMark] = useState(stored.mark);
  const [label, setLabel] = useState(stored.label);
  const [takes, setTakes] = useState<AgentsTake>(stored.takes);
  const [commands, setCommands] = useState(stored.commands);
  const [limit, setLimit] = useState(String(stored.limit));
  const [answer, setAnswer] = useState(stored.answer);
  const trimmed = label.trim();
  const limitNumber = Number(limit);
  const limitOk =
    Number.isInteger(limitNumber) && limitNumber >= 1 && limitNumber <= MAX_COMMAND_LIMIT;
  const changed =
    mark !== stored.mark ||
    trimmed !== stored.label ||
    takes !== stored.takes ||
    commands !== stored.commands ||
    (limitOk && limitNumber !== stored.limit) ||
    answer !== stored.answer;
  const valid = trimmed !== "" && limitOk;
  const save = useMutation({
    mutationFn: () =>
      editWorkingWithOthers(repository.id, {
        mark_ready: mark,
        ready_label: trimmed,
        agents_take: takes,
        commands,
        command_limit: limitNumber,
        answer_on_forge: answer,
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
          if (changed && valid && !save.isPending) save.mutate();
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
        <section
          aria-labelledby="commands-title"
          data-testid="issue-commands"
          className="flex flex-col gap-5 border-t border-hairline pt-5"
        >
          <div className="flex flex-col gap-1">
            <h3 id="commands-title" className="m-0 text-md font-semibold text-text">
              Commands in issue comments
            </h3>
            <p className="m-0 text-sm text-muted">
              {`On ${forge}, only people with a role on the repository may assign an issue, but anyone who commented on it may be assigned. This server assigns them with its own token, so helpers who only comment can take an issue.`}
            </p>
          </div>
          <div className="flex items-start justify-between gap-4">
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="text-base font-medium text-text">
                Take issues with /take, /release and /done
              </span>
              <span className="text-sm text-muted">
                {`Read from new comments at each poll, as the first word of a line (not in a quote or a code block). /take assigns the commenter, /release takes them off, and /done asks you to close the issue, which only you can. Turn this on at the repository owner\u2019s server only. Who may use them: anyone who can comment on the issue, on a public repository everyone.`}
              </span>
            </div>
            <Switch
              label="Take issues with /take, /release and /done"
              checked={commands}
              onChange={setCommands}
            />
          </div>
          <Field
            label="Issues one login may hold"
            hint={`A login may have at most this many open issues taken with /take, from 1 to ${MAX_COMMAND_LIMIT}. /take also needs an open issue nobody took${
              mark ? `, and one that carries "${trimmed || DEFAULT_READY_LABEL}"` : ""
            }. You can always release from the card.`}
            error={limitOk ? undefined : `A number from 1 to ${MAX_COMMAND_LIMIT}.`}
          >
            <Input
              type="number"
              inputMode="numeric"
              min={1}
              max={MAX_COMMAND_LIMIT}
              value={limit}
              autoComplete="off"
              onChange={(e) => setLimit(e.target.value)}
            />
          </Field>
          <div className="flex items-start justify-between gap-4">
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="text-base font-medium text-text">{`Answer on ${forge}`}</span>
              <span className="text-sm text-muted">
                {`Writes the result of every command as a short comment on the issue, such as "Assigned to @alice.", and the same for what you do on the card: release, close, give back.`}
              </span>
            </div>
            <Switch label={`Answer on ${forge}`} checked={answer} onChange={setAnswer} />
          </div>
        </section>
        <div className="flex gap-2">
          <Button type="submit" size="sm" disabled={!changed || !valid || save.isPending}>
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
