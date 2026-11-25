/**
 * The "Stale handling" box of a repository on a forge (ADR 0032): after how many days the server releases an issue a
 * helper took and never opened a pull request for, reminds the author of a followed pull request whose last review asked
 * for changes and that nobody pushed to, and closes it. Each is a number of days, 0 turns it off. A card that is held,
 * and every card while all agents are paused, is left alone; each action the server takes is in the daily digest.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { editUpkeep, readUpkeep } from "../../api/calls/factory";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { DEFAULT_UPKEEP, type Upkeep } from "../../api/types/factory";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";

const MOST_DAYS = 365;

/** A number of days as the server takes it: a whole number from 0 to 365, or null. */
function days(text: string): number | null {
  const n = Number(text);
  return text.trim() !== "" && Number.isInteger(n) && n >= 0 && n <= MOST_DAYS ? n : null;
}

export function StaleHandling({ repository }: { repository: number }) {
  const upkeep = useQuery({
    queryKey: keys.upkeep(repository),
    queryFn: () => readUpkeep(repository),
  });
  return (
    <Box title="Stale handling" testId="stale-handling" strong>
      {upkeep.data ? (
        <Form key={JSON.stringify(upkeep.data)} repository={repository} stored={upkeep.data} />
      ) : upkeep.isError ? (
        <Notice tone="error">The stale handling could not be read. Try again.</Notice>
      ) : (
        <SkeletonLines lines={3} />
      )}
    </Box>
  );
}

function Form({ repository, stored }: { repository: number; stored: Upkeep }) {
  const queryClient = useQueryClient();
  const [release, setRelease] = useState(String(stored.release_after_days));
  const [remind, setRemind] = useState(String(stored.stale_after_days));
  const [close, setClose] = useState(String(stored.close_after_days));
  const [r, s, c] = [days(release), days(remind), days(close)];
  const valid = r !== null && s !== null && c !== null;
  const order = valid && s > 0 && c > 0 && c <= s;
  const changed =
    valid &&
    (r !== stored.release_after_days ||
      s !== stored.stale_after_days ||
      c !== stored.close_after_days);
  const save = useMutation({
    mutationFn: () =>
      editUpkeep(repository, {
        release_after_days: r ?? DEFAULT_UPKEEP.release_after_days,
        stale_after_days: s ?? DEFAULT_UPKEEP.stale_after_days,
        close_after_days: c ?? DEFAULT_UPKEEP.close_after_days,
      }),
    onSuccess: (next) => {
      queryClient.setQueryData(keys.upkeep(repository), next);
    },
  });
  const field = (
    label: string,
    value: string,
    set: (v: string) => void,
    hint: string,
    error?: string,
  ) => (
    <Field
      label={label}
      hint={hint}
      error={days(value) === null ? `A number from 0 to ${MOST_DAYS}.` : error}
    >
      <Input
        type="number"
        inputMode="numeric"
        min={0}
        max={MOST_DAYS}
        value={value}
        autoComplete="off"
        onChange={(e) => {
          save.reset();
          set(e.target.value);
        }}
      />
    </Field>
  );
  return (
    <form
      className="flex max-w-xl flex-col gap-5"
      onSubmit={(e) => {
        e.preventDefault();
        if (changed && !order && !save.isPending) save.mutate();
      }}
    >
      <p className="m-0 text-base text-secondary">
        Once a day, before the digest, the server gives back what nobody works on. It never touches
        a card that is held, nor anything while all agents are paused, and the digest lists every
        action. 0 turns one off.
      </p>
      {field(
        "Release an issue taken for (days)",
        release,
        setRelease,
        "A helper took it, with /take or by being assigned, and no pull request of theirs is followed: they are taken off it, with a reply on the issue, and anyone may take it again.",
      )}
      {field(
        "Remind after (days)",
        remind,
        setRemind,
        "A followed pull request whose last review asked for changes saw no push: its author gets one reminder comment.",
      )}
      {field(
        "Close after (days)",
        close,
        setClose,
        "The same pull request is closed with a polite comment and its issue released.",
        order ? "Must be more than the days it is reminded after." : undefined,
      )}
      <div className="flex gap-2">
        <Button type="submit" size="sm" disabled={!changed || order || save.isPending}>
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
