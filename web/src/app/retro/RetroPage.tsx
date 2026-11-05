/**
 * The Retro page: the Retros of the days, the errors of the one chosen grouped by kind, the proposals the Retro
 * analyst made with their status and what their trials found, and under them the lessons it learned (`Lessons`).
 * "Run now" is `POST /v1/retros`, and a day whose analysis failed is made again from the banner that says so
 * (`POST /v1/retros/{date}/retry`); the server says when a Retro ends with `retro_done` and when a trial or a
 * proposal changes with `trial_done` and `proposal_status`, which `live/invalidation.ts` answers by reading again.
 *
 * The days are the list and the chosen day is the detail, so a person reads one day at a time: what was left out of
 * it, what went wrong, and what the Retro wants changed. The page fills the window and each column scrolls on its own,
 * because a day of errors is long and the list of days should stay where it was left. `?day=` names the day to open on
 * and `?proposal=` the one to land on, which is how the Inbox leads here from a proposal it cannot show all of.
 */
import { ArrowClockwiseIcon, WarningCircleIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { retro, retros, retryRetro, startRetro } from "../../api/calls/improve";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Proposal, ReportError, RetroView } from "../../api/types/improve";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { dayName } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { ListCard, ListDetail } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { Body, PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard, SkeletonList } from "../../ui/page/Skeleton";
import { Lessons } from "./Lessons";
import {
  cardsLine,
  groupErrors,
  origin,
  ownKind,
  plural,
  proposalTag,
  retroTag,
  type TagSpec,
  trialLine,
} from "./model";
import { Diff, Region } from "./parts";

export function RetroPage() {
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: keys.retros, queryFn: retros });
  const [picked, setPicked] = useState<string | null>(null);
  const [params] = useSearchParams();
  // The day a link came in with, else the latest; one picked from the list answers neither.
  const date = picked ?? params.get("day") ?? list.data?.[0]?.date ?? null;
  // The proposal a link came in for, which the day is scrolled to rather than read from its top.
  const wanted = params.get("proposal");
  const opened = useQuery({
    queryKey: keys.retro(date ?? ""),
    queryFn: () => retro(date as string),
    enabled: date !== null,
  });
  const run = useMutation({
    mutationFn: startRetro,
    onSuccess: async (view) => {
      setPicked(view.date);
      await queryClient.invalidateQueries({ queryKey: keys.retros });
    },
  });

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title="Retro"
        context="Help Moebius self-improve."
        actions={
          <Button size="sm" disabled={run.isPending} onClick={() => run.mutate()}>
            {run.isPending ? "Running" : "Run now"}
          </Button>
        }
      />
      {list.isPending ? (
        <ListDetail
          list={<SkeletonList rows={4} message="Reading the Retros" />}
          detail={<SkeletonCard lines={4} />}
        />
      ) : list.isError ? (
        // A read that failed is not a server that has run no Retro yet.
        <ReadFailed
          error={list.error}
          fallback="The Retros could not be read. Try again."
          busy={list.isFetching}
          onRetry={() => void list.refetch()}
        />
      ) : (
        <ListDetail
          list={
            list.data && list.data.length > 0 ? (
              list.data.map((each) => (
                <ListCard
                  key={each.date}
                  title={dayName(each.date)}
                  tags={[retroTag(each.status)]}
                  line={`${plural(each.errors, "error", "errors")} · ${plural(each.proposals, "proposal", "proposals")}`}
                  alert={each.status === "failed"}
                  selected={date === each.date}
                  onClick={() => setPicked(each.date)}
                />
              ))
            ) : (
              <Empty
                title="No day read yet"
                body="A Retro reads the day's runs and writes down what went wrong."
              />
            )
          }
          detail={
            <>
              {run.error && (
                <Region label="The Retro did not start">
                  <Notice tone="error">{sayError(run.error, "The Retro did not start.")}</Notice>
                </Region>
              )}
              {opened.data ? (
                <Day view={opened.data} wanted={wanted} />
              ) : date === null ? (
                <Empty
                  title="No Retro yet"
                  body={
                    <>
                      The first comes at <span className="font-mono">retro_at</span>, or start one
                      now.
                    </>
                  }
                />
              ) : opened.isPending ? (
                <SkeletonCard lines={5} message="Reading the Retro" />
              ) : opened.error ? (
                <Notice tone="error">
                  {sayError(opened.error, "Could not read the Retro. Try again.")}
                </Notice>
              ) : (
                <Empty title="Nothing came back" body="The Retro answered without a report." />
              )}
              <Lessons />
            </>
          }
        />
      )}
    </div>
  );
}

/** One day: what was left out, its errors by kind and its proposals. */
/** The day a person is reading. `wanted` is the proposal a link came in for. */
function Day({ view, wanted }: { view: RetroView; wanted: string | null }) {
  const queryClient = useQueryClient();
  const groups = groupErrors(view.report.errors);
  const proposals = view.proposals ?? [];
  const warnings = view.warnings ?? [];
  const retry = useMutation({
    mutationFn: retryRetro,
    onSuccess: async (view) => {
      await queryClient.invalidateQueries({ queryKey: keys.retros });
      await queryClient.invalidateQueries({ queryKey: keys.retro(view.date) });
    },
  });
  return (
    <>
      {warnings.length > 0 && (
        <Box
          title="Left out"
          aside={
            <span className="tabular-nums">
              {`${plural(warnings.length, "part", "parts")} of the day`}
            </span>
          }
          className="min-w-0"
        >
          <div className="flex flex-col gap-4">
            {warnings.map((warning) => (
              <div
                key={warning}
                className="flex items-center gap-4 rounded-control bg-attention-fill p-4 text-attention-text"
              >
                <WarningCircleIcon className="size-4 shrink-0" aria-hidden="true" />
                <p className="m-0 min-w-0 flex-1 break-words text-base">{warning}</p>
                {view.status === "failed" ? (
                  // The banner is the warning's own fill, so the button of it needs its border to read as a button.
                  <Button
                    size="sm"
                    variant="attention"
                    className="ring-1 ring-attention-border ring-inset"
                    disabled={retry.isPending}
                    onClick={() => retry.mutate(view.date)}
                  >
                    <ArrowClockwiseIcon className="size-4" aria-hidden="true" />
                    {retry.isPending ? "Running" : "Retry"}
                  </Button>
                ) : null}
              </div>
            ))}
            {retry.error && (
              <Notice tone="error">
                {sayError(retry.error, "Could not run the Retro again. Try again.")}
              </Notice>
            )}
          </div>
        </Box>
      )}
      <Box title="Errors" className="min-w-0">
        {groups.length === 0 ? (
          <Body>No errors that day.</Body>
        ) : (
          <div className="flex flex-col gap-6">
            {groups.map((group) => (
              <ErrorGroup key={group.kind} label={group.label} errors={group.errors} />
            ))}
          </div>
        )}
      </Box>
      <Box
        title="Proposals"
        aside={
          proposals.length > 0 ? (
            <span className="tabular-nums">{`${proposals.length} from this day`}</span>
          ) : undefined
        }
        className="min-w-0"
      >
        {proposals.length === 0 ? (
          <Body>None yet.</Body>
        ) : (
          <div className="flex flex-col gap-4">
            {proposals.map((proposal) => (
              <ProposalCard
                key={proposal.id}
                proposal={proposal}
                aimed={String(proposal.id) === wanted}
              />
            ))}
          </div>
        )}
      </Box>
    </>
  );
}

/** The errors of one kind: how many, then each with where it happened. */
function ErrorGroup({ label, errors }: { label: string; errors: ReportError[] }) {
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <h3 className="m-0 break-words text-base font-semibold text-text tabular-nums">
        {`${label} · ${errors.length}`}
      </h3>
      <div className="flex flex-col gap-4">
        {errors.map((error, index) => (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: the errors are a fixed report, in order
            key={index}
            className="flex min-w-0 flex-col gap-4 rounded-card bg-sunken p-4"
          >
            {origin(error) !== "" && (
              <span className="break-words font-mono text-sm text-secondary tabular-nums">
                {origin(error)}
              </span>
            )}
            <p className="m-0 min-w-0 break-words text-base leading-normal text-text whitespace-pre-wrap">
              {error.text}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}

/** A proposal: its title and status, the agents it changes, its branch, what it says it does, the cards it
 * addresses and what the trials found, or the diff of a change to Moebius's own configuration. The one a link from
 * the Inbox named is aimed at — the day is scrolled to it rather than read from its top. */
function ProposalCard({ proposal, aimed }: { proposal: Proposal; aimed?: boolean }) {
  const status = proposalTag(proposal.status);
  const own = ownKind(proposal.kind);
  const box = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (aimed) box.current?.scrollIntoView({ behavior: "instant", block: "start" });
  }, [aimed]);
  return (
    <article ref={box} className="flex min-w-0 flex-col gap-4 rounded-card bg-sunken p-4">
      <h3 className="m-0 min-w-0 break-words text-base font-semibold text-text">
        {proposal.title}
      </h3>
      <div className="flex flex-wrap gap-2">
        <Tag tone={status.tone}>{status.label}</Tag>
        {own && (
          <code className="break-words rounded-full bg-code px-2 py-1 font-mono text-xs text-text">
            {own}
          </code>
        )}
        {proposal.agents?.map((agent) => (
          <Tag key={agent}>{agent}</Tag>
        ))}
      </div>
      <span className="min-w-0 break-words font-mono text-sm text-secondary">
        {proposal.branch}
      </span>
      {proposal.body && (
        <p className="m-0 min-w-0 break-words text-base leading-normal text-text whitespace-pre-wrap">
          {proposal.body}
        </p>
      )}
      {proposal.cards && proposal.cards.length > 0 && (
        <span className="min-w-0 break-words text-sm text-secondary tabular-nums">
          {cardsLine(proposal.cards)}
        </span>
      )}
      {proposal.trials?.map((trial, index) => {
        const line = trialLine(trial);
        return (
          <TrialRow
            // biome-ignore lint/suspicious/noArrayIndexKey: the trials are stored in order and never change
            key={index}
            outcome={line.outcome}
            card={line.card}
            reason={trial.reason}
          />
        );
      })}
      {proposal.diff && <Diff text={proposal.diff} />}
    </article>
  );
}

/** One trial: what came of it, the card it was tried on, and why. */
function TrialRow({ outcome, card, reason }: { outcome: TagSpec; card: string; reason: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-4 sm:flex-row sm:items-center">
      <Tag tone={outcome.tone}>{outcome.label}</Tag>
      <span className="shrink-0 break-words text-sm text-secondary tabular-nums">{card}</span>
      <p className="m-0 min-w-0 flex-1 break-words text-base text-text">{reason}</p>
    </div>
  );
}
