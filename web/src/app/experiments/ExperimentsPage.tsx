/**
 * The Experiments page: the research the server has, a form for a new brief, and for the one chosen its baseline and
 * the levers tried, one row each. A brief is stored as a draft (`POST /v1/research`), then a person confirms it,
 * which lets it run in the trial window, or starts it now; a running research can be stopped, and the brief of a
 * draft is edited in place (`PUT /v1/research/{id}`). The server says when something changed with `research_status`
 * and `experiment_done`, which `live/invalidation.ts` answers by reading again.
 *
 * The forms of a brief are `BriefForms.tsx`, the brief of the one chosen `BriefBox.tsx`, and its baseline and levers
 * `Levers.tsx`.
 *
 * The page fills the window and each column scrolls on its own: a research with its baseline and its levers is read on
 * its own, and the list of briefs should stay where it was left.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useSearchParams } from "react-router";
import { snapshot } from "../../api/calls/board";
import { actOnResearch, createResearch, editResearch, researches } from "../../api/calls/improve";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Research } from "../../api/types/improve";
import { Button } from "../../ui/components/Button";
import { plural } from "../../ui/lib/format";
import { Empty } from "../../ui/page/Empty";
import { ListCard, ListDetail } from "../../ui/page/List";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { useToast } from "../../ui/page/Toast";
import { ImproveTabs } from "../improve/ImproveTabs";
import { BriefBox } from "./BriefBox";
import { EditBrief, NewBrief } from "./BriefForms";
import { Baseline, Levers } from "./Levers";
import { emptyForm, type Form, SUGGESTED_BRIEFS, statusTag } from "./model";

export function ExperimentsPage() {
  const queryClient = useQueryClient();
  const toast = useToast();
  const list = useQuery({ queryKey: keys.research, queryFn: researches });
  const boards = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  // The research is in the address (`?research=`), so a reload and a link keep it; with none the first is read.
  const [params, setParams] = useSearchParams();
  const pickedParam = Number(params.get("research"));
  const picked = Number.isInteger(pickedParam) && pickedParam > 0 ? pickedParam : null;
  const setPicked = (id: number | null) =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        if (id === null) next.delete("research");
        else next.set("research", String(id));
        return next;
      },
      { replace: true },
    );
  const [composing, setComposing] = useState(false);
  /** The brief the form starts from, when the page offered one. */
  const [starting, setStarting] = useState<Form | null>(null);
  const [editing, setEditing] = useState(false);
  const repositories = boards.data?.boards.map((board) => board.repository) ?? [];
  const repositoryName = (research: Research) =>
    repositories.find((each) => each.id === research.repository_id)?.name ?? "";

  const chosen = list.data?.find((each) => each.id === picked) ?? list.data?.[0];
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.research });
  const answered = useMutation({
    mutationFn: (request: () => Promise<Research>) => request(),
    onSuccess: async (research) => {
      setComposing(false);
      setStarting(null);
      setEditing(false);
      setPicked(research.id);
      await refresh();
    },
    onError: (error) => toast({ kind: "error", text: sayError(error, "The request was refused.") }),
  });

  const header = (
    <PageHeader
      title="Improve"
      tabs={<ImproveTabs value="improve/experiments" />}
      primary={
        <Button
          size="sm"
          disabled={composing}
          onClick={() => {
            setComposing(true);
            setStarting(null);
            setEditing(false);
          }}
        >
          <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
          New brief
        </Button>
      }
    />
  );
  if (list.isPending) {
    return (
      <>
        {header}
        <SkeletonCard lines={4} message="Reading the research" />
      </>
    );
  }
  if (list.error || !list.data) {
    return (
      <>
        {header}
        <ReadFailed
          error={list.error}
          fallback="Could not read the research."
          busy={list.isFetching}
          onRetry={() => void list.refetch()}
        />
      </>
    );
  }

  const startWith = (brief: Pick<Form, "goal" | "measure" | "better">) => {
    setStarting({ ...emptyForm(), ...brief });
    setComposing(true);
    setEditing(false);
  };

  // With no research there is nothing to list beside the page, so the page is one card that says how to begin.
  if (list.data.length === 0) {
    return (
      <>
        {header}
        {composing ? (
          <NewBrief
            repositories={repositories}
            busy={answered.isPending}
            starting={starting}
            onStore={(repository, brief) =>
              answered.mutate(() => createResearch(repository, brief))
            }
            onCancel={() => {
              setComposing(false);
              setStarting(null);
            }}
          />
        ) : (
          <Empty
            title="No research yet"
            body="A brief names one number and the command that prints it. Moebius then tries one lever at a time and keeps what moves the number."
            action={
              <div className="flex flex-wrap gap-2">
                {SUGGESTED_BRIEFS.map((brief) => (
                  <Button
                    key={brief.goal}
                    size="sm"
                    variant="secondary"
                    onClick={() => startWith(brief)}
                  >
                    {brief.goal}
                  </Button>
                ))}
              </div>
            }
          />
        )}
      </>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {header}
      <ListDetail
        list={list.data.map((research) => {
          const kept = (research.experiments ?? []).filter((each) => each.kept).length;
          const count = research.experiments?.length ?? 0;
          return (
            <ListCard
              key={research.id}
              title={research.goal}
              tags={[statusTag(research.status)]}
              line={`${repositoryName(research)} · ${plural(count, "experiment", "experiments")} · ${kept} kept`}
              selected={!composing && chosen?.id === research.id}
              onClick={() => {
                setPicked(research.id);
                setComposing(false);
                setEditing(false);
              }}
            />
          );
        })}
        detail={
          composing ? (
            <NewBrief
              repositories={repositories}
              busy={answered.isPending}
              starting={starting}
              onStore={(repository, brief) =>
                answered.mutate(() => createResearch(repository, brief))
              }
              onCancel={() => setComposing(false)}
            />
          ) : chosen ? (
            <>
              {editing && chosen.status === "draft" ? (
                <EditBrief
                  research={chosen}
                  busy={answered.isPending}
                  onCancel={() => setEditing(false)}
                  onSave={(brief) => answered.mutate(() => editResearch(chosen.id, brief))}
                />
              ) : (
                <BriefBox
                  research={chosen}
                  repository={repositoryName(chosen)}
                  busy={answered.isPending}
                  onEdit={() => setEditing(true)}
                  onDeleted={() => setPicked(null)}
                  onAct={(action) => answered.mutate(() => actOnResearch(chosen.id, action))}
                />
              )}
              <Baseline research={chosen} />
              <Levers research={chosen} />
            </>
          ) : null
        }
      />
    </div>
  );
}
