/**
 * The Retro area of Settings: the Retros of the days, the errors of the one chosen grouped by kind and the proposals
 * the Retro analyst made with their status and what their trials found (the lessons it learned are the Lessons area).
 * It has no page header of its own, only the area's title with "Run now" at its right; a refusal is a toast.
 * "Run now" is `POST /v1/retros`, and a day whose analysis failed is made again from the banner that says so
 * (`POST /v1/retros/{date}/retry`); the server says when a Retro ends with `retro_done` and when a trial or a
 * proposal changes with `trial_done` and `proposal_status`, which `live/invalidation.ts` answers by reading again.
 *
 * The days are the list and the chosen day is the detail, so a person reads one day at a time: what was left out of
 * it, what went wrong, and what the Retro wants changed. The page fills the window and each column scrolls on its own,
 * because a day of errors is long and the list of days should stay where it was left. `?day=` names the day to open on
 * and `?proposal=` the one to land on, which is how the Inbox leads here from a proposal it cannot show all of.
 */

import { ArrowClockwiseIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router";
import { retro, retros, retryRetro, startRetro } from "../../api/calls/improve";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Proposal, ReportError, RetroView } from "../../api/types/improve";
import { Markdown } from "../../ui/chat/Markdown";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { dayName } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { ListCard, ListDetail } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { Body } from "../../ui/page/PageHeader";
import { Property } from "../../ui/page/Property";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard, SkeletonList } from "../../ui/page/Skeleton";
import { useToast } from "../../ui/page/Toast";
import { AreaTitle } from "../settings/parts";
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
import { Diff } from "./parts";

export function RetroPage() {
  const queryClient = useQueryClient();
  const toast = useToast();
  const list = useQuery({ queryKey: keys.retros, queryFn: retros });
  const [params, setParams] = useSearchParams();
  // The day is in the address, so a reload and a link keep it; with none the latest is read.
  const date = params.get("day") ?? list.data?.[0]?.date ?? null;
  const setPicked = (day: string) =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        next.set("day", day);
        next.delete("proposal");
        return next;
      },
      { replace: true },
    );
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
    onError: (error) => toast({ kind: "error", text: sayError(error, "The Retro did not start.") }),
  });

  return (
    <div className="flex h-full min-h-0 flex-col">
      <AreaTitle
        title="Retro"
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
            opened.data ? (
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
            )
          }
        />
      )}
    </div>
  );
}

/** The day a person is reading. `wanted` is the proposal a link came in for. */
function Day({ view, wanted }: { view: RetroView; wanted: string | null }) {
  const queryClient = useQueryClient();
  const toast = useToast();
  const groups = groupErrors(view.report.errors);
  const proposals = view.proposals ?? [];
  const estimates = view.report.estimates ?? [];
  const warnings = view.warnings ?? [];
  const retry = useMutation({
    mutationFn: retryRetro,
    onSuccess: async (view) => {
      await queryClient.invalidateQueries({ queryKey: keys.retros });
      await queryClient.invalidateQueries({ queryKey: keys.retro(view.date) });
    },
    onError: (error) =>
      toast({ kind: "error", text: sayError(error, "Could not run the Retro again. Try again.") }),
  });
  return (
    <>
      <section
        aria-label={`Properties of the Retro of ${dayName(view.date)}`}
        className="flex flex-col gap-1"
      >
        <Property name="Day">{dayName(view.date)}</Property>
        <Property name="Status">{retroTag(view.status).label}</Property>
        <Property name="Errors">{String(view.report.errors.length)}</Property>
        <Property name="Proposals">{String(proposals.length)}</Property>
        <Property name="Left out">{String(warnings.length)}</Property>
      </section>
      {warnings.length > 0 && (
        <Box title="Left out" className="min-w-0">
          {/* One row for what was left out, with the one Retry beside it: the retry runs the whole day, not a warning. */}
          <div className="flex flex-col items-start gap-3">
            <div className="flex min-w-0 flex-col gap-2 text-secondary">
              {warnings.map((warning) => (
                <p key={warning} className="m-0 min-w-0 break-words text-base">
                  {warning}
                </p>
              ))}
            </div>
            {view.status === "failed" ? (
              <Button
                size="sm"
                variant="secondary"
                disabled={retry.isPending}
                onClick={() => retry.mutate(view.date)}
              >
                <ArrowClockwiseIcon className="size-4" aria-hidden="true" />
                {retry.isPending ? "Running" : "Retry"}
              </Button>
            ) : null}
          </div>
        </Box>
      )}
      <Box title="Errors" className="min-w-0">
        {groups.length === 0 ? (
          <Body>No errors that day.</Body>
        ) : (
          <div className="flex flex-col gap-4">
            {groups.map((group) => (
              <ErrorGroup key={group.kind} label={group.label} errors={group.errors} />
            ))}
          </div>
        )}
      </Box>
      {estimates.length > 0 && (
        <Box title="Estimates furthest off" className="min-w-0">
          <ul className="m-0 flex list-none flex-col gap-2 p-0">
            {estimates.map((miss) => (
              <li key={miss.card} className="text-base">
                {`Card ${miss.card}, ${miss.title}: ${miss.estimate} ${miss.estimate === 1 ? "point" : "points"}, took ${miss.tokens_ratio}× the tokens its points are worth (${miss.minutes} min)`}
              </li>
            ))}
          </ul>
        </Box>
      )}
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
          <div className="flex flex-col divide-y divide-hairline">
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
    <div className="flex min-w-0 flex-col gap-1">
      <h3 className="m-0 break-words text-base font-semibold text-text tabular-nums">
        {`${label} · ${errors.length}`}
      </h3>
      <div className="flex flex-col divide-y divide-hairline">
        {errors.map((error, index) => (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: the errors are a fixed report, in order
            key={index}
            className="flex min-w-0 flex-col gap-1 py-2"
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
    <article ref={box} className="flex min-w-0 flex-col gap-3 py-4 first:pt-0 last:pb-0">
      <h3 className="m-0 min-w-0 break-words text-base font-semibold text-text">
        {proposal.title}
      </h3>
      <div className="flex flex-wrap gap-2">
        <Tag tone={status.tone}>{status.label}</Tag>
        {own && (
          <code className="break-words rounded-control bg-code px-2 py-1 font-mono text-xs text-text">
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
      {proposal.body && <Markdown content={proposal.body} keepNewlines className="max-w-prose" />}
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
    <div className="flex min-w-0 flex-col gap-2 sm:min-h-9 sm:flex-row sm:items-center sm:gap-4">
      <Tag tone={outcome.tone}>{outcome.label}</Tag>
      <span className="shrink-0 break-words text-sm text-secondary tabular-nums">{card}</span>
      <p className="m-0 min-w-0 flex-1 break-words text-base text-text">{reason}</p>
    </div>
  );
}
