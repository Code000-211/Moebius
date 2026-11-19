/**
 * The two dialogs of the Cycles pages: the form that makes or edits a cycle (its name, dates and description) and
 * the question that closes one. `CyclesPage.tsx` and `CyclePage.tsx` open them.
 */

import { useMutation } from "@tanstack/react-query";
import { useId, useState } from "react";
import { addCycle, closeCycle, editCycle } from "../../api/calls/cycles";
import { sayError } from "../../api/errorText";
import type { Carry, CycleFields, CycleView } from "../../api/types/cycles";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Input } from "../../ui/components/Input";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { nextCycle } from "./model";

/** The dialog that makes a cycle of the project or edits one: its name, its first and its last day. */
export function CycleDialog({
  editing,
  project,
  onClose,
  onDone,
}: {
  editing: CycleView | "new" | null;
  project: number;
  onClose: () => void;
  onDone: () => void;
}) {
  return (
    <Dialog open={editing !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        {editing !== null && (
          <CycleForm
            key={editing === "new" ? "new" : editing.id}
            project={project}
            editing={editing === "new" ? null : editing}
            onDone={() => {
              onDone();
              onClose();
            }}
            onCancel={onClose}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function CycleForm({
  project,
  editing,
  onDone,
  onCancel,
}: {
  project: number;
  editing: CycleView | null;
  onDone: () => void;
  onCancel: () => void;
}) {
  const [fields, setFields] = useState<CycleFields>(
    editing
      ? { name: editing.name, starts_on: editing.starts_on, ends_on: editing.ends_on }
      : { name: "", starts_on: "", ends_on: "" },
  );
  const save = useMutation({
    mutationFn: () => {
      const body = { ...fields, name: fields.name.trim() };
      return editing === null ? addCycle(project, body) : editCycle(editing.id, body);
    },
    onSuccess: onDone,
  });
  const ready =
    fields.name.trim() !== "" &&
    fields.starts_on !== "" &&
    fields.ends_on !== "" &&
    !save.isPending;
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) save.mutate();
      }}
    >
      <DialogHeader>
        <DialogTitle>{editing === null ? "New cycle" : "Edit cycle"}</DialogTitle>
        <DialogDescription>
          A time box of this project. Put its cards in from the card's properties.
        </DialogDescription>
      </DialogHeader>
      <Field label="Name">
        <Input
          value={fields.name}
          placeholder="Cycle 12"
          autoComplete="off"
          onChange={(event) => setFields({ ...fields, name: event.target.value })}
        />
      </Field>
      <Field label="First day">
        <Input
          type="date"
          value={fields.starts_on}
          onChange={(event) => setFields({ ...fields, starts_on: event.target.value })}
        />
      </Field>
      <Field label="Last day">
        <Input
          type="date"
          value={fields.ends_on}
          onChange={(event) => setFields({ ...fields, ends_on: event.target.value })}
        />
      </Field>
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save the cycle. </span>
          {sayError(save.error, "Try again.")}
        </Notice>
      )}
      <DialogFooter>
        <Button type="button" variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={!ready}>
          {save.isPending ? "Saving" : editing === null ? "Make cycle" : "Save"}
        </Button>
      </DialogFooter>
    </form>
  );
}

/** The dialog that closes a cycle: the person chooses where the cards that are not done go. */
export function CloseDialog({
  cycle,
  all,
  onClose,
  onDone,
}: {
  cycle: CycleView;
  all: CycleView[];
  onClose: () => void;
  onDone: () => void;
}) {
  const name = useId();
  const others = all
    .filter((c) => c.id !== cycle.id && c.closed_at === null)
    .sort((a, b) => a.starts_on.localeCompare(b.starts_on) || a.id - b.id);
  const next = nextCycle(all, cycle);
  const [carry, setCarry] = useState<"next" | "out">(next ? "next" : "out");
  const [to, setTo] = useState<number | undefined>(next?.id);
  const unfinished = cycle.cards - cycle.done;
  const close = useMutation({
    mutationFn: () => {
      const body: Carry =
        carry === "out" || to === undefined ? { carry: "out" } : { carry: "next", to };
      return closeCycle(cycle.id, body);
    },
    onSuccess: onDone,
  });
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            close.mutate();
          }}
        >
          <DialogHeader>
            <DialogTitle>Close {cycle.name}</DialogTitle>
            <DialogDescription>
              {unfinished === 0
                ? "Every card is done. The cycle keeps its cards as its record."
                : `${unfinished} ${unfinished === 1 ? "card is" : "cards are"} not done. Choose where ${unfinished === 1 ? "it goes" : "they go"}; the cycle keeps its cards as its record.`}
            </DialogDescription>
          </DialogHeader>
          <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
            <legend className="sr-only">Cards that are not done</legend>
            <label className="flex items-center gap-2 text-base text-text">
              <input
                type="radio"
                name={name}
                checked={carry === "next"}
                disabled={others.length === 0}
                onChange={() => setCarry("next")}
              />
              Move them to
              <select
                aria-label="Next cycle"
                className="h-8 rounded-control bg-field px-2 text-base text-text"
                value={to ?? ""}
                disabled={others.length === 0}
                onChange={(event) => {
                  setTo(Number(event.target.value));
                  setCarry("next");
                }}
              >
                {others.length === 0 && <option value="">No open cycle</option>}
                {others.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-2 text-base text-text">
              <input
                type="radio"
                name={name}
                checked={carry === "out"}
                onChange={() => setCarry("out")}
              />
              Take them out of any cycle
            </label>
          </fieldset>
          {close.isError && (
            <Notice tone="error">
              <span className="font-semibold">Couldn't close the cycle. </span>
              {sayError(close.error, "Try again.")}
            </Notice>
          )}
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={close.isPending}>
              {close.isPending ? "Closing" : "Close cycle"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
