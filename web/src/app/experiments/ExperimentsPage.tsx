/**
 * The Experiments page: the research the server has, a form for a new brief, and for the one chosen its baseline and
 * the experiments, one for each lever the Autoresearcher proposed. A brief is stored as a draft (`POST /v1/research`),
 * then a person confirms it, which lets it run in the trial window, or starts it now; a running research can be
 * stopped. The brief of a draft has a pencil that edits it in place (`PUT /v1/research/{id}`). The server says when
 * something changed with `research_status` and `experiment_done`, which `live/invalidation.ts` answers by reading again.
 */
import { PencilSimpleIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { snapshot } from "../../api/calls/board";
import { actOnResearch, createResearch, editResearch, researches } from "../../api/calls/improve";
import { keys } from "../../api/keys";
import type { Research } from "../../api/types/improve";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Loader } from "../../ui/components/Loader";
import {
  Body,
  Box,
  CELL,
  Chip,
  Field,
  ListCard,
  ListDetail,
  Notice,
  PageHeader,
  sayError,
  Table,
} from "../kit/Kit";
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
  statusTag,
} from "./model";

export function ExperimentsPage() {
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: keys.research, queryFn: researches });
  const boards = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const [picked, setPicked] = useState<number | null>(null);
  const [composing, setComposing] = useState(false);
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
      setEditing(false);
      setPicked(research.id);
      await refresh();
    },
    onError: (error) => setFailed(sayError(error, "The request was refused.")),
  });

  const header = (
    <PageHeader
      breadcrumb="Improve"
      title="Experiments"
      context="A number to improve, a command that measures it, and the levers tried."
      actions={
        <Button
          size="sm"
          disabled={composing}
          onClick={() => {
            setComposing(true);
            setFailed(null);
          }}
        >
          New brief
        </Button>
      }
    />
  );
  if (list.isPending) {
    return (
      <>
        {header}
        <Loader message="Reading the research…" className="py-double" />
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

  return (
    <>
      {header}
      <ListDetail
        list={
          list.data.length === 0 ? (
            <Box title="Research">
              <Body>None yet.</Body>
            </Box>
          ) : (
            list.data.map((research) => {
              const kept = (research.experiments ?? []).filter((e) => e.kept).length;
              const count = research.experiments?.length ?? 0;
              return (
                <ListCard
                  key={research.id}
                  title={research.goal}
                  tags={[statusTag(research.status)]}
                  line={`${repositoryName(research)} · ${count} ${count === 1 ? "experiment" : "experiments"} · ${kept} kept`}
                  chosen={!composing && chosen?.id === research.id}
                  onClick={() => {
                    setPicked(research.id);
                    setComposing(false);
                    setEditing(false);
                    setFailed(null);
                  }}
                />
              );
            })
          )
        }
        detail={
          <>
            {failed && !editing && (
              <Box title="The request was refused">
                <Notice tone="error">{failed}</Notice>
              </Box>
            )}
            {composing ? (
              <NewBrief
                repositories={repositories}
                busy={answered.isPending}
                onStore={(repository, brief) =>
                  answered.mutate(() => createResearch(repository, brief))
                }
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
                    onAct={(action) => answered.mutate(() => actOnResearch(chosen.id, action))}
                  />
                )}
                <Box title="Baseline">
                  {chosen.baseline === null ? (
                    <Body>Not measured yet.</Body>
                  ) : (
                    <span className="text-[24px] font-bold text-high">
                      {number(chosen.baseline)}
                    </span>
                  )}
                </Box>
                <Levers research={chosen} />
              </>
            ) : (
              <Box title="No research yet">
                <Body>Press "New brief" to say what to improve and how to measure it.</Body>
              </Box>
            )}
          </>
        }
      />
    </>
  );
}

/** The fields every brief has: goal, measure, direction and token budget. */
function BriefFields({ form, onChange }: { form: Form; onChange: (form: Form) => void }) {
  return (
    <>
      <Field label="Goal">
        {(id) => (
          <Input
            id={id}
            placeholder="What should get better?"
            value={form.goal}
            onChange={(event) => onChange({ ...form, goal: event.target.value })}
          />
        )}
      </Field>
      <Field label="Measure">
        {(id) => (
          <Input
            id={id}
            className="font-mono"
            placeholder="cargo bench | tail -1"
            value={form.measure}
            onChange={(event) => onChange({ ...form, measure: event.target.value })}
          />
        )}
      </Field>
      <fieldset className="flex gap-base border-0 p-0" aria-label="Direction">
        <Chip
          active={form.better === "lower"}
          onClick={() => onChange({ ...form, better: "lower" })}
        >
          Lower is better
        </Chip>
        <Chip
          active={form.better === "higher"}
          onClick={() => onChange({ ...form, better: "higher" })}
        >
          Higher is better
        </Chip>
      </fieldset>
      <Field label="Token budget" className="max-w-[240px] flex-none">
        {(id) => (
          <Input
            id={id}
            inputMode="numeric"
            placeholder={DEFAULT_BUDGET}
            value={form.budget}
            onChange={(event) => onChange({ ...form, budget: event.target.value })}
          />
        )}
      </Field>
    </>
  );
}

/** The form of a new brief. */
function NewBrief({
  repositories,
  busy,
  onStore,
}: {
  repositories: Array<{ id: number; name: string }>;
  busy: boolean;
  onStore: (repository: number, brief: NonNullable<ReturnType<typeof briefOf>>) => void;
}) {
  const [form, setForm] = useState<Form>(emptyForm);
  // The first repository until one is chosen, as the snapshot may arrive after the form opens.
  const repository = form.repository ?? repositories[0]?.id ?? null;
  const brief = briefOf({ ...form, repository });
  return (
    <Box title="New brief">
      <div className="flex flex-col gap-half">
        <span className="text-xs font-semibold text-low">Repository</span>
        <fieldset className="flex flex-wrap gap-base border-0 p-0" aria-label="Repository">
          {repositories.map((each) => (
            <Chip
              key={each.id}
              active={repository === each.id}
              onClick={() => setForm({ ...form, repository: each.id })}
            >
              {each.name}
            </Chip>
          ))}
        </fieldset>
      </div>
      <BriefFields form={form} onChange={setForm} />
      <Body>
        A shell command run in a worktree. Its result is the last number it prints; no number is a
        failed measurement.
      </Body>
      <div>
        <Button
          size="sm"
          variant="outline"
          disabled={brief === null || busy}
          onClick={() => brief && repository !== null && onStore(repository, brief)}
        >
          Store the brief
        </Button>
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
      <BriefFields form={form} onChange={setForm} />
      {error && <Notice tone="error">{error}</Notice>}
      <div className="flex gap-base">
        <Button size="sm" disabled={brief === null || busy} onClick={() => brief && onSave(brief)}>
          Save
        </Button>
        <Button size="sm" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </Box>
  );
}

/** The brief of the research chosen, with the buttons its status allows. */
function BriefBox({
  research,
  repository,
  busy,
  onEdit,
  onAct,
}: {
  research: Research;
  repository: string;
  busy: boolean;
  onEdit: () => void;
  onAct: (action: "confirm" | "start" | "stop") => void;
}) {
  const status = statusTag(research.status);
  const direction = research.better === "higher" ? "Higher is better" : "Lower is better";
  return (
    <Box title="Brief">
      <div className="flex flex-wrap items-center gap-base">
        <h3 className="mr-auto break-words text-sm font-semibold text-high">{research.goal}</h3>
        {research.status === "draft" && (
          <button
            type="button"
            aria-label="Edit the brief"
            title="Edit the brief"
            onClick={onEdit}
            className="rounded-sm p-half text-low hover:bg-card hover:text-high"
          >
            <PencilSimpleIcon className="size-icon-sm" />
          </button>
        )}
        <Tag tone={status.tone}>{status.label}</Tag>
      </div>
      <code className="break-words font-mono text-xs text-high">{research.measure}</code>
      <span className="font-mono text-xs text-low">
        {repository} · {direction} · {research.tokens ?? 0} of {research.token_budget} tokens used
      </span>
      {research.reason && <Body>{research.reason}</Body>}
      <div className="flex gap-base">
        {actions(research.status).map(({ label, action, primary }) => (
          <Button
            key={label}
            size="sm"
            variant={primary ? "default" : "outline"}
            disabled={busy}
            onClick={() => onAct(action)}
          >
            {label}
          </Button>
        ))}
      </div>
    </Box>
  );
}

/** The table of the levers tried. */
function Levers({ research }: { research: Research }) {
  const experiments = research.experiments ?? [];
  if (experiments.length === 0) {
    return (
      <Box title="Experiments">
        <Body>No lever has been tried yet.</Body>
      </Box>
    );
  }
  return (
    <Box title="Experiments">
      <Table head={["Lever", "Branch", "Value", "Change", "Result", "Reason"]}>
        {experiments.map((experiment) => {
          const shown = result(experiment);
          const moved =
            experiment.value !== null && research.baseline !== null
              ? change(research.baseline, experiment.value)
              : null;
          return (
            <tr key={experiment.id}>
              <td className={`${CELL} font-semibold text-high`}>{experiment.title}</td>
              <td className={`${CELL} break-all font-mono text-xs text-low`}>
                {experiment.branch}
              </td>
              <td className={CELL}>{experiment.value === null ? "—" : number(experiment.value)}</td>
              <td className={CELL}>{moved ?? "—"}</td>
              <td className={CELL}>
                <Tag tone={shown.tone}>{shown.label}</Tag>
              </td>
              <td className={`${CELL} break-words`}>{experiment.reason ?? ""}</td>
            </tr>
          );
        })}
      </Table>
    </Box>
  );
}
