/**
 * The Experiments page: the research the server has, a form for a new brief, and for the one chosen its baseline and
 * the levers tried, one row each. A brief is stored as a draft (`POST /v1/research`), then a person confirms it,
 * which lets it run in the trial window, or starts it now; a running research can be stopped, and the brief of a
 * draft is edited in place (`PUT /v1/research/{id}`). The server says when something changed with `research_status`
 * and `experiment_done`, which `live/invalidation.ts` answers by reading again.
 *
 * The page fills the window and each column scrolls on its own: a research with its baseline and its levers is read on
 * its own, and the list of briefs should stay where it was left.
 */
import { PencilSimpleIcon, PlusIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { snapshot } from "../../api/calls/board";
import { actOnResearch, createResearch, editResearch, researches } from "../../api/calls/improve";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Research } from "../../api/types/improve";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { IconButton } from "../../ui/components/IconButton";
import { Input } from "../../ui/components/Input";
import { plural } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Empty } from "../../ui/page/Empty";
import { Field } from "../../ui/page/Field";
import { CELL, ListCard, ListDetail, Table } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { Body, PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { DeleteBrief } from "./DeleteBrief";
import {
  actions,
  briefOf,
  change,
  DEFAULT_BUDGET,
  emptyForm,
  type Form,
  formOf,
  number,
  result,
  SUGGESTED_BRIEFS,
  statusTag,
} from "./model";
import { Region } from "./parts";

export function ExperimentsPage() {
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: keys.research, queryFn: researches });
  const boards = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const [picked, setPicked] = useState<number | null>(null);
  const [composing, setComposing] = useState(false);
  /** The brief the form starts from, when the page offered one. */
  const [starting, setStarting] = useState<Form | null>(null);
  const [editing, setEditing] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const repositories = boards.data?.boards.map((board) => board.repository) ?? [];
  const repositoryName = (research: Research) =>
    repositories.find((each) => each.id === research.repository_id)?.name ?? "";

  const chosen = list.data?.find((each) => each.id === picked) ?? list.data?.[0];
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.research });
  const answered = useMutation({
    mutationFn: (request: () => Promise<Research>) => request(),
    onSuccess: async (research) => {
      setFailed(null);
      setComposing(false);
      setStarting(null);
      setEditing(false);
      setPicked(research.id);
      await refresh();
    },
    onError: (error) => setFailed(sayError(error, "The request was refused.")),
  });

  const header = (
    <PageHeader
      title="Experiments"
      context="Try one change at a time against a reference point."
      actions={
        <Button
          size="sm"
          disabled={composing}
          onClick={() => {
            setComposing(true);
            setStarting(null);
            setEditing(false);
            setFailed(null);
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
        <Notice tone="error">{sayError(list.error, "Could not read the research.")}</Notice>
      </>
    );
  }

  const startWith = (brief: Pick<Form, "goal" | "measure" | "better">) => {
    setStarting({ ...emptyForm(), ...brief });
    setComposing(true);
    setEditing(false);
    setFailed(null);
  };

  // With no research there is nothing to list beside the page, so the page is one card that says how to begin.
  if (list.data.length === 0) {
    return (
      <>
        {header}
        {failed && (
          <Region label="The request was refused">
            <Notice tone="error">{failed}</Notice>
          </Region>
        )}
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
                setFailed(null);
              }}
            />
          );
        })}
        detail={
          <>
            {failed && !editing && (
              <Region label="The request was refused">
                <Notice tone="error">{failed}</Notice>
              </Region>
            )}
            {composing ? (
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
                    error={failed}
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
            ) : null}
          </>
        }
      />
    </div>
  );
}

/** The fields every brief has: what should get better, the command that measures it, which way of that number is
 * the goal, and the budget. The words here are the ones the form shows; `goal`, `measure` and `better` are what the
 * request calls them. */
function BriefFields({ form, onChange }: { form: Form; onChange: (form: Form) => void }) {
  return (
    <>
      <Field label="What should get better">
        {(id) => (
          <Input
            id={id}
            placeholder="Cut the clean build time of the host"
            value={form.goal}
            onChange={(event) => onChange({ ...form, goal: event.target.value })}
          />
        )}
      </Field>
      <Field
        label="Measurement"
        hint="A shell command run in a worktree. Its result is the last number it prints."
      >
        {(id) => (
          <Input
            id={id}
            className="font-mono text-sm"
            placeholder="cargo bench | tail -1"
            value={form.measure}
            onChange={(event) => onChange({ ...form, measure: event.target.value })}
          />
        )}
      </Field>
      <div className="grid gap-4 md:grid-cols-2">
        <fieldset className="m-0 min-w-0 border-0 p-0">
          <legend className="m-0 mb-2 p-0 text-sm font-semibold text-secondary">Goal</legend>
          <div className="flex flex-wrap gap-2">
            <Chip
              selected={form.better === "lower"}
              onClick={() => onChange({ ...form, better: "lower" })}
            >
              Lower is better
            </Chip>
            <Chip
              selected={form.better === "higher"}
              onClick={() => onChange({ ...form, better: "higher" })}
            >
              Higher is better
            </Chip>
          </div>
        </fieldset>
        <Field
          label="Token budget"
          hint="What the research may spend before it stops."
          className="min-w-0"
        >
          {(id) => (
            <Input
              id={id}
              inputMode="numeric"
              className="tabular-nums"
              placeholder={DEFAULT_BUDGET}
              value={form.budget}
              onChange={(event) => onChange({ ...form, budget: event.target.value })}
            />
          )}
        </Field>
      </div>
    </>
  );
}

/** The form of a new brief. */
function NewBrief({
  repositories,
  busy,
  onStore,
  onCancel,
  starting,
}: {
  repositories: Array<{ id: number; name: string }>;
  busy: boolean;
  onStore: (repository: number, brief: NonNullable<ReturnType<typeof briefOf>>) => void;
  onCancel: () => void;
  /** A brief to open the form with, when the empty page offered one. */
  starting?: Form | null;
}) {
  const [form, setForm] = useState<Form>(starting ?? emptyForm);
  // The first repository until one is chosen, as the snapshot may arrive after the form opens.
  const repository = form.repository ?? repositories[0]?.id ?? null;
  const brief = briefOf({ ...form, repository });
  return (
    <Box title="New brief">
      <div className="flex flex-col gap-4">
        <fieldset className="m-0 min-w-0 border-0 p-0">
          <legend className="m-0 mb-2 p-0 text-sm font-semibold text-secondary">Repository</legend>
          {repositories.length === 0 ? (
            <p className="m-0 break-words text-sm text-secondary">
              No repository yet. Add one under Projects first.
            </p>
          ) : (
            <div className="flex flex-wrap gap-2">
              {repositories.map((each) => (
                <Chip
                  key={each.id}
                  selected={repository === each.id}
                  onClick={() => setForm({ ...form, repository: each.id })}
                >
                  {each.name}
                </Chip>
              ))}
            </div>
          )}
        </fieldset>
        <BriefFields form={form} onChange={setForm} />
        <Body>The brief is stored as a draft. Nothing runs until it is confirmed and started.</Body>
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={brief === null || busy}
            onClick={() => brief && repository !== null && onStore(repository, brief)}
          >
            {busy ? "Saving" : "Store the brief"}
          </Button>
          <Button size="sm" variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </div>
    </Box>
  );
}

/** The brief of a draft grown into its editor. */
function EditBrief({
  research,
  busy,
  error,
  onCancel,
  onSave,
}: {
  research: Research;
  busy: boolean;
  error: string | null;
  onCancel: () => void;
  onSave: (brief: NonNullable<ReturnType<typeof briefOf>>) => void;
}) {
  const [form, setForm] = useState<Form>(() => formOf(research));
  const brief = briefOf(form);
  return (
    <Box title="Brief">
      <div className="flex flex-col gap-4">
        <BriefFields form={form} onChange={setForm} />
        {error && <Notice tone="error">{error}</Notice>}
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={brief === null || busy}
            onClick={() => brief && onSave(brief)}
          >
            {busy ? "Saving" : "Save"}
          </Button>
          <Button size="sm" variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </div>
    </Box>
  );
}

/** The brief of the research chosen: what it is after, how far it has got, and what a person can do with it now. */
function BriefBox({
  research,
  repository,
  busy,
  onEdit,
  onDeleted,
  onAct,
}: {
  research: Research;
  repository: string;
  busy: boolean;
  onEdit: () => void;
  onDeleted: () => void;
  onAct: (action: "confirm" | "start" | "stop") => void;
}) {
  const status = statusTag(research.status);
  const direction = research.better === "higher" ? "Higher is better" : "Lower is better";
  const possible = actions(research.status);
  return (
    <Box
      title="Brief"
      actions={
        research.status === "draft" ? (
          <IconButton
            icon={PencilSimpleIcon}
            aria-label="Edit the brief"
            title="Edit the brief"
            onClick={onEdit}
          />
        ) : undefined
      }
    >
      <div className="flex flex-col gap-4">
        <h3 className="m-0 min-w-0 break-words text-lg font-semibold text-text">{research.goal}</h3>
        <div className="flex flex-wrap items-center gap-2">
          <Tag tone={status.tone}>{status.label}</Tag>
          <span className="text-sm text-secondary tabular-nums">
            {repository} · {direction} · {research.tokens ?? 0} of {research.token_budget} tokens
            used
          </span>
        </div>
        <span className="min-w-0 break-words rounded-control bg-sunken px-3 py-2 font-mono text-sm text-text">
          {research.measure}
        </span>
        {research.reason && <Body>{research.reason}</Body>}
        <div className="flex flex-wrap gap-2">
          {possible.map(({ label, action, primary }) => (
            <Button
              key={label}
              size="sm"
              variant={primary ? "primary" : "secondary"}
              disabled={busy}
              onClick={() => onAct(action)}
            >
              {label}
            </Button>
          ))}
          <DeleteBrief research={research} onDeleted={onDeleted} />
        </div>
      </div>
    </Box>
  );
}

/** The number the levers are compared with. */
function Baseline({ research }: { research: Research }) {
  return (
    <Box title="Baseline">
      {research.baseline === null ? (
        <Body>Not measured yet. The first number the measure gives becomes it.</Body>
      ) : (
        <div className="flex flex-col gap-2">
          <span className="text-2xl font-semibold text-text tabular-nums">
            {number(research.baseline)}
          </span>
          <p className="m-0 max-w-prose text-sm text-secondary">
            The number the measure gave with nothing changed. Each lever is judged against it.
          </p>
        </div>
      )}
    </Box>
  );
}

/** The table of the levers tried: what was changed, what it measured, and what came of it. */
function Levers({ research }: { research: Research }) {
  const experiments = research.experiments ?? [];
  if (experiments.length === 0) {
    return (
      <Box title="Experiments" aside="One lever each">
        <Body>No lever has been tried yet.</Body>
      </Box>
    );
  }
  return (
    <Box
      title="Experiments"
      aside={
        <span className="tabular-nums">{plural(experiments.length, "lever", "levers")} tried</span>
      }
    >
      <Table head={["Lever", "Value", "Change", "Result"]}>
        {experiments.map((experiment) => {
          const shown = result(experiment);
          const moved =
            experiment.value !== null && research.baseline !== null
              ? change(research.baseline, experiment.value)
              : null;
          return (
            <tr key={experiment.id}>
              <td className={CELL}>
                <div className="flex min-w-0 flex-col gap-2">
                  <span className="min-w-0 break-words font-semibold text-text">
                    {experiment.title}
                  </span>
                  <code className="min-w-0 break-words font-mono text-sm text-secondary">
                    {experiment.branch}
                  </code>
                </div>
              </td>
              <td className={`${CELL} whitespace-nowrap tabular-nums`}>
                {experiment.value === null ? "—" : number(experiment.value)}
              </td>
              <td className={`${CELL} whitespace-nowrap tabular-nums`}>{moved ?? "—"}</td>
              <td className={CELL}>
                <div className="flex min-w-0 flex-col gap-2">
                  <Tag tone={shown.tone}>{shown.label}</Tag>
                  {experiment.reason && (
                    <span className="min-w-0 break-words text-sm text-secondary">
                      {experiment.reason}
                    </span>
                  )}
                </div>
              </td>
            </tr>
          );
        })}
      </Table>
    </Box>
  );
}
