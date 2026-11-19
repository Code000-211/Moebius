/**
 * The forms of a brief on the Experiments page: the fields every brief has, the form of a new brief and the editor a
 * draft's brief grows into. The page stores them with `POST /v1/research` and `PUT /v1/research/{id}`.
 */
import { useState } from "react";
import type { Research } from "../../api/types/improve";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Body } from "../../ui/page/PageHeader";
import { briefOf, defaultAmount, emptyForm, type Form, formOf, withKind } from "./model";

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
        <fieldset className="m-0 min-w-0 border-0 p-0">
          <legend className="m-0 mb-2 p-0 text-sm font-semibold text-secondary">Limited by</legend>
          <div className="flex flex-wrap gap-2">
            <Chip
              selected={form.kind === "experiments"}
              onClick={() => onChange(withKind(form, "experiments"))}
            >
              Experiments
            </Chip>
            <Chip
              selected={form.kind === "tokens"}
              onClick={() => onChange(withKind(form, "tokens"))}
            >
              Tokens
            </Chip>
          </div>
        </fieldset>
        <Field
          label={form.kind === "experiments" ? "Experiments" : "Tokens"}
          hint={
            form.kind === "experiments"
              ? "The most levers measured against the baseline before it stops."
              : "What the research may spend before it stops."
          }
          className="min-w-0 md:col-span-2"
        >
          {(id) => (
            <Input
              id={id}
              inputMode="numeric"
              className="tabular-nums"
              placeholder={defaultAmount(form.kind)}
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
export function NewBrief({
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
export function EditBrief({
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
