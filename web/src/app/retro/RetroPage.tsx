/**
 * The Retro page: the Retros of the days, the errors of the one chosen grouped by kind, the proposals the Retro
 * analyst made with their status and what their trials found, and under them the lessons it learned (`Lessons`).
 * "Run the Retro now" is `POST /v1/retros`; the server says when a Retro ends with `retro_done` and when a trial or a
 * proposal changes with `trial_done` and `proposal_status`, which `live/invalidation.ts` answers by reading again.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { retro, retros, startRetro } from "../../api/calls/improve";
import { keys } from "../../api/keys";
import type { Proposal, RetroView } from "../../api/types/improve";
import { CodeBlock } from "../../ui/chat/CodeBlock";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { Body, Box, ListCard, ListDetail, Notice, PageHeader, sayError } from "../kit/Kit";
import { Lessons } from "./Lessons";
import {
  cardsLine,
  groupErrors,
  origin,
  ownKind,
  plural,
  proposalTag,
  retroTag,
  trialLine,
} from "./model";

export function RetroPage() {
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: keys.retros, queryFn: retros });
  const [picked, setPicked] = useState<string | null>(null);
  // The latest day until one is picked.
  const date = picked ?? list.data?.[0]?.date ?? null;
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
    <>
      <PageHeader
        breadcrumb="Improve"
        title="Retro"
        context="Errors of each day, and the changes the Retro proposes to avoid them."
        actions={
          <Button size="sm" disabled={run.isPending} onClick={() => run.mutate()}>
            Run the Retro now
          </Button>
        }
      />
      {list.isPending ? (
        <Loader message="Reading the Retros…" className="py-double" />
      ) : (
        <ListDetail
          list={
            list.data && list.data.length > 0 ? (
              list.data.map((each) => (
                <ListCard
                  key={each.date}
                  title={each.date}
                  tags={[retroTag(each.status)]}
                  line={`${plural(each.errors, "error", "errors")} · ${plural(each.proposals, "proposal", "proposals")}`}
                  chosen={date === each.date}
                  onClick={() => setPicked(each.date)}
                />
              ))
            ) : (
              <Box title="Retros">
                <Body>None yet.</Body>
              </Box>
            )
          }
          detail={
            <>
              {run.error && (
                <Box title="The Retro did not start">
                  <Notice tone="error">{sayError(run.error, "The Retro did not start.")}</Notice>
                </Box>
              )}
              {opened.data ? (
                <Day view={opened.data} />
              ) : date === null ? (
                <Box title="No Retro yet">
                  <Body>The first runs at retro_at, or press "Run the Retro now".</Body>
                </Box>
              ) : opened.error ? (
                <Notice tone="error">{sayError(opened.error, "Could not read the Retro.")}</Notice>
              ) : (
                <Box title="Retro">
                  <Body>Reading the Retro…</Body>
                </Box>
              )}
              <Lessons />
            </>
          }
        />
      )}
    </>
  );
}

/** One day: what was left out, its errors by kind and its proposals. */
function Day({ view }: { view: RetroView }) {
  const groups = groupErrors(view.report.errors);
  const proposals = view.proposals ?? [];
  return (
    <>
      {view.warnings && view.warnings.length > 0 && (
        <Box title="Left out">
          {view.warnings.map((warning) => (
            <p key={warning} className="break-words text-sm text-amber">
              {warning}
            </p>
          ))}
        </Box>
      )}
      <Box title={`Errors of ${view.date}`}>
        {groups.length === 0 ? (
          <Body>No errors that day.</Body>
        ) : (
          groups.map((group) => (
            <div key={group.kind} className="flex flex-col gap-half">
              <h3 className="text-sm font-semibold text-high">
                {group.label} · {group.errors.length}
              </h3>
              {group.errors.map((error, index) => (
                <div
                  // biome-ignore lint/suspicious/noArrayIndexKey: the errors are a fixed report, in order
                  key={index}
                  className="flex flex-col gap-half"
                >
                  <span className="font-mono text-xs text-low">{origin(error)}</span>
                  <span className="whitespace-pre-wrap break-words text-sm text-normal">
                    {error.text}
                  </span>
                </div>
              ))}
            </div>
          ))
        )}
      </Box>
      <Box title="Proposals">
        {proposals.length === 0 ? (
          <Body>The Retro proposed nothing.</Body>
        ) : (
          proposals.map((proposal) => <ProposalCard key={proposal.id} proposal={proposal} />)
        )}
      </Box>
    </>
  );
}

/** A proposal: its title and status, the agents it changes, its branch, what it says it does, the cards it addresses and what the trials found, or the diff of a change to Moebius's own configuration. */
function ProposalCard({ proposal }: { proposal: Proposal }) {
  const status = proposalTag(proposal.status);
  const own = ownKind(proposal.kind);
  return (
    <article className="flex flex-col gap-half rounded-lg border bg-lane p-base">
      <div className="flex flex-wrap items-center gap-half">
        <h3 className="mr-auto break-words text-sm font-semibold text-high">{proposal.title}</h3>
        {proposal.agents?.map((agent) => (
          <Tag key={agent}>{agent}</Tag>
        ))}
        {own && <Tag>{own}</Tag>}
        <Tag tone={status.tone}>{status.label}</Tag>
      </div>
      <span className="break-all font-mono text-xs text-low">{proposal.branch}</span>
      {proposal.body && (
        <p className="whitespace-pre-wrap break-words text-sm text-normal">{proposal.body}</p>
      )}
      {proposal.cards && proposal.cards.length > 0 && (
        <span className="font-mono text-xs text-low">{cardsLine(proposal.cards)}</span>
      )}
      {proposal.trials?.map((trial, index) => {
        const line = trialLine(trial);
        return (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: the trials are stored in order and never change
            key={index}
            className="flex flex-wrap items-center gap-base"
          >
            <Tag tone={line.outcome.tone}>{line.outcome.label}</Tag>
            <span className="font-mono text-xs text-low">{line.card}</span>
            <span className="min-w-0 flex-1 break-words text-sm text-normal">{trial.reason}</span>
          </div>
        );
      })}
      {proposal.diff && (
        <CodeBlock language="diff" code={proposal.diff}>
          {proposal.diff}
        </CodeBlock>
      )}
    </article>
  );
}
