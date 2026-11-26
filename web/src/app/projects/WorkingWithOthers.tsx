/**
 * The "Working with others" box of a repository on a forge (ADR 0027): what this server does so that people who each run
 * a server on the same repository do not do the same issue twice, through the forge alone. It marks the issues that are
 * ready with a label (only the owner's server should), and it says which issues this server's agents take: every approved
 * one, or only those that carry the label and nobody took. A second part, "Commands in issue comments", lets people who
 * may comment on an issue but not assign it take, release and finish issues with `/take`, `/release` and `/done`, which the
 * server does for them with its own token. A third, "Pull requests of others", says whether a pull request the Reviewer
 * approved is merged by the server. A fourth, "Merging", holds what every merge of the server passes: the paths that never merge
 * without a person, the measurement a change needs, the review rules read from the default branch and the
 * people whose measurements count; a freeze stops the whole repository. Saved with one button, except the freeze.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  editWorkingWithOthers,
  freezeRepository,
  unfreezeRepository,
} from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { AgentsTake, Repository } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field, Select, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Switch } from "../../ui/page/Switch";
import { FrozenNotice } from "../board/FrozenNotice";
import { forgeName } from "../board/model";

/** The label until a person gives another. */
export const DEFAULT_READY_LABEL = "Ready for agent";

/** How many issues a login may hold that it took with `/take`, until a person sets another number. */
export const DEFAULT_COMMAND_LIMIT = 3;
const MAX_COMMAND_LIMIT = 50;

export const DEFAULT_TESTING_LABEL = "ready-for-testing";
export const DEFAULT_REVIEW_BRIEF = ["REVIEW.md"];

/** What the server never merges without a person, whatever the owner adds. */
export const BUILT_IN_LOCKED = [
  ".github/**",
  "CMakeLists.txt, *.cmake, build.sh, Dockerfile and package manifests",
  "AGENTS.md, CLAUDE.md",
];

/** The lines of a box, one entry each, without blanks and repeats. */
const lines = (text: string): string[] => [
  ...new Set(
    text
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line !== ""),
  ),
];
const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);

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
    merge: repository.merge_reviewed ?? true,
    measure: repository.require_measurement ?? false,
    locked: repository.locked_paths ?? [],
    brief: repository.review_brief ?? DEFAULT_REVIEW_BRIEF,
    testing: repository.testing_label ?? DEFAULT_TESTING_LABEL,
    measurers: repository.measurers ?? [],
    free: repository.measurement_free_paths ?? [],
  };
  const [mark, setMark] = useState(stored.mark);
  const [label, setLabel] = useState(stored.label);
  const [takes, setTakes] = useState<AgentsTake>(stored.takes);
  const [commands, setCommands] = useState(stored.commands);
  const [limit, setLimit] = useState(String(stored.limit));
  const [answer, setAnswer] = useState(stored.answer);
  const [merge, setMerge] = useState(stored.merge);
  const [measure, setMeasure] = useState(stored.measure);
  const [locked, setLocked] = useState(stored.locked.join("\n"));
  const [brief, setBrief] = useState(stored.brief.join("\n"));
  const [testing, setTesting] = useState(stored.testing);
  const [measurers, setMeasurers] = useState(stored.measurers.join("\n"));
  const [free, setFree] = useState(stored.free.join("\n"));
  const trimmed = label.trim();
  const limitNumber = Number(limit);
  const limitOk =
    Number.isInteger(limitNumber) && limitNumber >= 1 && limitNumber <= MAX_COMMAND_LIMIT;
  const testingTrimmed = testing.trim();
  const changed =
    measure !== stored.measure ||
    !same(lines(locked), stored.locked) ||
    !same(lines(brief), stored.brief) ||
    testingTrimmed !== stored.testing ||
    !same(lines(measurers), stored.measurers) ||
    !same(lines(free), stored.free) ||
    mark !== stored.mark ||
    trimmed !== stored.label ||
    takes !== stored.takes ||
    commands !== stored.commands ||
    (limitOk && limitNumber !== stored.limit) ||
    answer !== stored.answer ||
    merge !== stored.merge;
  const valid = trimmed !== "" && limitOk && testingTrimmed !== "";
  const save = useMutation({
    mutationFn: () =>
      editWorkingWithOthers(repository.id, {
        mark_ready: mark,
        ready_label: trimmed,
        agents_take: takes,
        commands,
        command_limit: limitNumber,
        answer_on_forge: answer,
        merge_reviewed: merge,
        require_measurement: measure,
        locked_paths: lines(locked),
        review_brief: lines(brief),
        testing_label: testingTrimmed,
        measurers: lines(measurers),
        measurement_free_paths: lines(free),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
    },
  });
  const freeze = useMutation({
    mutationFn: () =>
      repository.frozen
        ? unfreezeRepository(repository.id, "unfrozen from the repository's settings")
        : freezeRepository(repository.id, "frozen from the repository's settings"),
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
        <section
          aria-labelledby="pull-requests-title"
          data-testid="external-pull-requests"
          className="flex flex-col gap-5 border-t border-hairline pt-5"
        >
          <h3 id="pull-requests-title" className="m-0 text-md font-semibold text-text">
            Pull requests of others
          </h3>
          <div className="flex items-start justify-between gap-4">
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="text-base font-medium text-text">
                Merge external pull requests the reviewer approves
              </span>
              <span className="text-sm text-muted">
                {`When the author of a pull request that closes an issue writes /done, the Reviewer reads it, and an approved one whose checks have passed is merged. Foreign code then lands without a person looking. Off, it is approved on ${forge} and waits in Ready to merge for you.`}
              </span>
            </div>
            <Switch
              label="Merge external pull requests the reviewer approves"
              checked={merge}
              onChange={setMerge}
            />
          </div>
        </section>
        <section
          aria-labelledby="merging-title"
          data-testid="merging"
          className="flex flex-col gap-5 border-t border-hairline pt-5"
        >
          <div className="flex flex-col gap-1">
            <h3 id="merging-title" className="m-0 text-md font-semibold text-text">
              Merging
            </h3>
            <p className="m-0 text-sm text-muted">
              Every merge the server makes passes the same gate: the checks of the head are green,
              the paths below are not touched, the measurement is there when it is required, the
              repository is not frozen and the card is not held. Refused, the card waits in Ready to
              merge with the reason in your Inbox.
            </p>
          </div>
          <div className="flex items-start justify-between gap-4">
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="text-base font-medium text-text">Require a measurement</span>
              <span className="text-sm text-muted">
                A change merges only with a measurement a registered measurer posted for its head,
                the label no-measurement on its issue, or when it changes only measurement-free
                paths.
              </span>
            </div>
            <Switch label="Require a measurement" checked={measure} onChange={setMeasure} />
          </div>
          <Field
            label="Measurement-free paths"
            hint="One glob per line, read as the locked paths are. A change that touches only these needs no measurement; one path more and it does."
          >
            <TextArea
              mono
              rows={2}
              value={free}
              placeholder="notes/backlog/closed/**"
              onChange={(e) => setFree(e.target.value)}
            />
          </Field>
          <Field
            label="Locked paths"
            hint={`One glob per line. A change that touches one never merges without a person. Always locked, besides these: ${BUILT_IN_LOCKED.join("; ")}.`}
          >
            <TextArea
              mono
              rows={3}
              value={locked}
              placeholder="kernels/**"
              onChange={(e) => setLocked(e.target.value)}
            />
          </Field>
          <Field
            label="Review brief"
            hint="One path per line, read from the default branch, never from the pull request. They are the repository's own review rules, given to the Reviewer; 40 KB at most."
          >
            <TextArea
              mono
              rows={2}
              value={brief}
              placeholder="REVIEW.md"
              onChange={(e) => setBrief(e.target.value)}
            />
          </Field>
          <Field
            label="Ready for testing"
            hint="Put on an issue whose code the Reviewer approved while its measurement is missing."
          >
            <Input
              value={testing}
              autoComplete="off"
              placeholder={DEFAULT_TESTING_LABEL}
              onChange={(e) => setTesting(e.target.value)}
            />
          </Field>
          <Field
            label="Measurers"
            hint={`One login on ${forge} per line. Stored and shown here; their "## Measurement" comments start a review round in a later release.`}
          >
            <TextArea
              mono
              rows={2}
              value={measurers}
              placeholder="alice"
              onChange={(e) => setMeasurers(e.target.value)}
            />
          </Field>
        </section>
        <section
          aria-labelledby="freeze-title"
          data-testid="freeze"
          className="flex flex-col gap-3 border-t border-hairline pt-5"
        >
          <h3 id="freeze-title" className="m-0 text-md font-semibold text-text">
            Freeze
          </h3>
          <FrozenNotice frozen={repository.frozen} />
          <p className="m-0 text-sm text-muted">
            {`A frozen repository starts no review round, merges nothing and starts no agent run. The label factory:freeze on ${forge} freezes it too. Two reverts in a week, a day's tokens over 80% and a failed nightly check freeze it by themselves. Only a person unfreezes it.`}
          </p>
          <div>
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={freeze.isPending}
              onClick={() => freeze.mutate()}
            >
              {repository.frozen ? "Unfreeze" : "Freeze this repository"}
            </Button>
          </div>
          {freeze.isError && (
            <Notice tone="error">
              {isApiError(freeze.error) ? freeze.error.message : "It could not be changed."}
            </Notice>
          )}
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
