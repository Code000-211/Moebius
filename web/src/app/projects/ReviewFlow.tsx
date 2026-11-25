/**
 * The "Review flow" box of a repository on a forge (ADR 0033): whether a followed pull request is reviewed once the
 * checks of its head have been green for a few minutes, without its author's /done, and after how many days a pull
 * request that names no issue is closed. A freeze, a hold and Pause all stop both; the round limit holds. Saved with one
 * button.
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
  const [w, c] = [whole(wait, MOST_MINUTES), whole(close, MOST_DAYS)];
  const prefixOk = prefix.trim() !== "" && prefix.length <= 30 && !/[,\n]/.test(prefix);
  const valid = w !== null && c !== null && prefixOk;
  const changed =
    valid &&
    (onGreen !== stored.review_on_green ||
      w !== stored.green_wait_minutes ||
      c !== stored.unbound_close_days ||
      statusOn !== stored.status_labels ||
      prefix.trim() !== stored.status_prefix);
  const save = useMutation({
    mutationFn: () =>
      editFlow(repository, {
        review_on_green: onGreen,
        green_wait_minutes: w ?? DEFAULT_FLOW.green_wait_minutes,
        unbound_close_days: c ?? DEFAULT_FLOW.unbound_close_days,
        status_labels: statusOn,
        status_prefix: prefix.trim(),
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
        read at most three times.
      </p>
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
