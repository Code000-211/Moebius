/**
 * The "Review flow" box of a repository on a forge (ADR 0033): whether a followed pull request is reviewed once the
 * checks of its head have been green for a few minutes, without its author's /done, and after how many days a pull
 * request that names no issue is closed. A freeze, a hold and Pause all stop both; the round limit, which the box sets
 * too (0 for none), holds. Saved with one
 * button. Also whether Moebius's own agents work on the forge as people do (ADR 0034): a draft pull request from the first
 * round and the verdict as a review of it.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { editFlow, readFlow } from "../../api/calls/factory";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { DEFAULT_FLOW, type Flow } from "../../api/types/factory";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { Switch } from "../../ui/page/Switch";

const MOST_MINUTES = 120;
const MOST_DAYS = 365;
const MOST_ROUNDS = 100;

/** A whole number from 0 to `most`, or null. */
function whole(text: string, most: number): number | null {
  const n = Number(text);
  return text.trim() !== "" && Number.isInteger(n) && n >= 0 && n <= most ? n : null;
}

export function ReviewFlow({ repository }: { repository: number }) {
  const flow = useQuery({
    queryKey: keys.flow(repository),
    queryFn: () => readFlow(repository),
  });
  return (
    <Box title="Review flow" testId="review-flow" strong>
      {flow.data ? (
        <Form key={JSON.stringify(flow.data)} repository={repository} stored={flow.data} />
      ) : flow.isError ? (
        <Notice tone="error">The review flow could not be read. Try again.</Notice>
      ) : (
        <SkeletonLines lines={3} />
      )}
    </Box>
  );
}

function Form({ repository, stored }: { repository: number; stored: Flow }) {
  const queryClient = useQueryClient();
  const [onGreen, setOnGreen] = useState(stored.review_on_green);
  const [wait, setWait] = useState(String(stored.green_wait_minutes));
  const [close, setClose] = useState(String(stored.unbound_close_days));
  const [statusOn, setStatusOn] = useState(stored.status_labels);
  const [prefix, setPrefix] = useState(stored.status_prefix);
  const [onForge, setOnForge] = useState(stored.work_on_forge);
  const [rounds, setRounds] = useState(String(stored.review_round_limit));
  const [w, c, r] = [
    whole(wait, MOST_MINUTES),
    whole(close, MOST_DAYS),
    whole(rounds, MOST_ROUNDS),
  ];
  const prefixOk = prefix.trim() !== "" && prefix.length <= 30 && !/[,\n]/.test(prefix);
  const valid = w !== null && c !== null && r !== null && prefixOk;
  const changed =
    valid &&
    (onGreen !== stored.review_on_green ||
      w !== stored.green_wait_minutes ||
      c !== stored.unbound_close_days ||
      statusOn !== stored.status_labels ||
      onForge !== stored.work_on_forge ||
      r !== stored.review_round_limit ||
      prefix.trim() !== stored.status_prefix);
  const save = useMutation({
    mutationFn: () =>
      editFlow(repository, {
        review_on_green: onGreen,
        green_wait_minutes: w ?? DEFAULT_FLOW.green_wait_minutes,
        unbound_close_days: c ?? DEFAULT_FLOW.unbound_close_days,
        status_labels: statusOn,
        status_prefix: prefix.trim(),
        work_on_forge: onForge,
        review_round_limit: r ?? DEFAULT_FLOW.review_round_limit,
      }),
    onSuccess: (next) => {
      queryClient.setQueryData(keys.flow(repository), next);
    },
  });
  return (
    <form
      className="flex max-w-xl flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault();
        if (changed && !save.isPending) save.mutate();
      }}
    >
      <p className="m-0 text-base text-secondary">
        What lets reviews and pull requests move without a click of yours. Nothing starts for a card
        that is held, in a frozen repository, or while all agents are paused, and a pull request is
        read at most as often as the round limit below says.
      </p>
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="text-base font-medium text-text">Work on the forge</span>
          <span className="text-sm text-muted">
            Moebius's own agents work the way a developer does: the pull request opens as a draft
            after the first round, with commits that say what was done, the Reviewer's verdict is a
            review with comments on the lines, each finding is answered, and the issue keeps only
            its status and the link. Takes effect on forges that have drafts.
          </span>
        </div>
        <Switch label="Work on the forge" checked={onForge} onChange={setOnForge} />
      </div>
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="text-base font-medium text-text">Review when the checks are green</span>
          <span className="text-sm text-muted">
            A pull request someone else opened is reviewed once its checks have been green for the
            minutes below with no new push. Its author's /done still works; whichever comes first
            starts the round.
          </span>
        </div>
        <Switch label="Review when the checks are green" checked={onGreen} onChange={setOnGreen} />
      </div>
      <Field
        label="Green for (minutes)"
        hint="How long the checks of a head must stay green. A push begins the wait again."
        error={w === null ? `A number from 0 to ${MOST_MINUTES}.` : undefined}
      >
        <Input
          type="number"
          inputMode="numeric"
          min={0}
          max={MOST_MINUTES}
          value={wait}
          autoComplete="off"
          onChange={(e) => {
            save.reset();
            setWait(e.target.value);
          }}
        />
      </Field>
      <Field
        label="Close a pull request that names no issue after (days)"
        hint="It is asked once to write Fixes #N, and gets the label unbound. A title or branch with a backlog item's ID, or #N of an issue its author is assigned to, binds it. 0 never closes one."
        error={c === null ? `A number from 0 to ${MOST_DAYS}.` : undefined}
      >
        <Input
          type="number"
          inputMode="numeric"
          min={0}
          max={MOST_DAYS}
          value={close}
          autoComplete="off"
          onChange={(e) => {
            save.reset();
            setClose(e.target.value);
          }}
        />
      </Field>
      <Field
        label="Review rounds per pull request"
        hint="How many times the Reviewer reads a pull request someone else opened before you are asked to look. Rounds you ask for yourself do not count. 0 is no limit."
        error={r === null ? `A number from 0 to ${MOST_ROUNDS}.` : undefined}
      >
        <Input
          type="number"
          inputMode="numeric"
          min={0}
          max={MOST_ROUNDS}
          value={rounds}
          autoComplete="off"
          onChange={(e) => {
            save.reset();
            setRounds(e.target.value);
          }}
        />
      </Field>
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="text-base font-medium text-text">Status labels on the forge</span>
          <span className="text-sm text-muted">
            Every open issue that has a card carries one label that says where it stands: its column
            (status:in-review), or taken, changes-requested, awaiting-measurement or frozen while
            those hold. A label removed by hand is put back at the next poll.
          </span>
        </div>
        <Switch label="Status labels on the forge" checked={statusOn} onChange={setStatusOn} />
      </div>
      <Field
        label="Status label prefix"
        hint="What a status label starts with. Labels without it are never touched."
        error={prefixOk ? undefined : "1 to 30 characters, no comma."}
      >
        <Input
          value={prefix}
          autoComplete="off"
          onChange={(e) => {
            save.reset();
            setPrefix(e.target.value);
          }}
        />
      </Field>
      <div className="flex gap-2">
        <Button type="submit" size="sm" disabled={!changed || save.isPending}>
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
  );
}
