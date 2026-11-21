/**
 * The dialog that makes a milestone or edits one: its name, its target date and its description. The Milestones page
 * opens it from the New button and from a milestone's Edit button.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { addMilestone, editMilestone } from "../../api/calls/milestones";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Milestone, MilestoneFields, MilestoneView } from "../../api/types/milestones";
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
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";

export function MilestoneDialog({
  editing,
  onClose,
  onMade,
}: {
  editing: MilestoneView | "new" | null;
  onClose: () => void;
  /** Called with a milestone just made, before the dialog closes. */
  onMade?: (milestone: Milestone) => void;
}) {
  const queryClient = useQueryClient();
  const initial: MilestoneFields =
    editing === null || editing === "new"
      ? { name: "", target_date: "", description: "" }
      : { name: editing.name, target_date: editing.target_date, description: editing.description };
  return (
    <Dialog open={editing !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        {editing !== null && (
          <MilestoneForm
            key={editing === "new" ? "new" : editing.id}
            initial={initial}
            id={editing === "new" ? null : editing.id}
            onDone={(saved) => {
              if (editing === "new") onMade?.(saved);
              void queryClient.invalidateQueries({ queryKey: keys.milestones });
              void queryClient.invalidateQueries({ queryKey: keys.cards });
              onClose();
            }}
            onCancel={onClose}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function MilestoneForm({
  initial,
  id,
  onDone,
  onCancel,
}: {
  initial: MilestoneFields;
  id: number | null;
  onDone: (saved: Milestone) => void;
  onCancel: () => void;
}) {
  const [fields, setFields] = useState(initial);
  const save = useMutation({
    mutationFn: () =>
      id === null
        ? addMilestone({ ...fields, name: fields.name.trim() })
        : editMilestone(id, { ...fields, name: fields.name.trim() }),
    onSuccess: onDone,
  });
  const ready = fields.name.trim() !== "" && fields.target_date !== "" && !save.isPending;
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) save.mutate();
      }}
    >
      <DialogHeader>
        <DialogTitle>{id === null ? "New milestone" : "Edit milestone"}</DialogTitle>
        <DialogDescription>
          A goal of the organisation. Put cards of any project in it from the card's properties.
        </DialogDescription>
      </DialogHeader>
      <Field label="Name">
        <Input
          value={fields.name}
          placeholder="Launch 2.0"
          autoComplete="off"
          onChange={(event) => setFields({ ...fields, name: event.target.value })}
        />
      </Field>
      <Field label="Target date">
        <Input
          type="date"
          value={fields.target_date}
          onChange={(event) => setFields({ ...fields, target_date: event.target.value })}
        />
      </Field>
      <Field label="Description">
        <TextArea
          value={fields.description}
          rows={3}
          onChange={(event) => setFields({ ...fields, description: event.target.value })}
        />
      </Field>
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save the milestone. </span>
          {sayError(save.error, "Try again.")}
        </Notice>
      )}
      <DialogFooter>
        <Button type="button" variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={!ready}>
          {save.isPending ? "Saving" : id === null ? "Make milestone" : "Save"}
        </Button>
      </DialogFooter>
    </form>
  );
}
