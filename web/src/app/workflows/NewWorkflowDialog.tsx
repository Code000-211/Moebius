/**
 * The "New workflow" dialog, which opens from the Workflows page's header and its empty state. It asks for a name —
 * the workflow's file name, so it is fixed once the file exists — and one sentence of what the workflow is for.
 *
 * The name is checked here, before the first save, because the server's `PUT /v1/workflows/{name}` takes any name:
 * the rules it must already meet are the safe name's (letters, digits, `-` and `_`, per `docs/reference/workflows.md`)
 * and not being taken, which is read off the workflows list. Confirming does not call the server: it opens the canvas
 * with a blank draft as router state, and the canvas's first save is what creates the file. The small race — two
 * people confirming the same name at once — stays, as the save the second one leads to is refused there.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { keys } from "../../api/keys";
import { workflows } from "../../api/workflows";
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
import { newWorkflowDraft } from "./canvas/model";

/** A file name under the harness configuration repository: letters, digits, `-` and `_`. */
const SAFE_NAME = /^[a-zA-Z0-9_-]+$/;

export function NewWorkflowDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const taken = useQuery({
    queryKey: keys.workflows,
    queryFn: workflows,
    enabled: open,
  });

  const trimmed = name.trim();
  const takenName = (taken.data ?? []).some((entry) => entry.name === trimmed);
  const problem = !SAFE_NAME.test(trimmed)
    ? "A workflow's name may hold only letters, digits, - and _."
    : takenName
      ? "A workflow with this name exists already."
      : null;
  const ready = trimmed !== "" && problem === null && !taken.isPending;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-4 p-6">
        <DialogHeader>
          <DialogTitle>New workflow</DialogTitle>
          <DialogDescription>
            The canvas opens with a blank graph; the first save writes the file.
          </DialogDescription>
        </DialogHeader>
        <Field
          label="Name"
          hint="The file name of the workflow, which does not change after it is saved."
          error={trimmed === "" || taken.isPending ? undefined : (problem ?? undefined)}
        >
          {(id) => (
            <Input
              id={id}
              mono
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          )}
        </Field>
        <Field label="Description">
          {(id) => (
            <TextArea
              id={id}
              value={description}
              onChange={(event) => setDescription(event.target.value)}
            />
          )}
        </Field>
        <DialogFooter>
          <Button variant="secondary" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="primary"
            data-testid="new-workflow-create"
            disabled={!ready}
            onClick={() => {
              navigate(`/workflows/${encodeURIComponent(trimmed)}/canvas`, {
                state: { draft: newWorkflowDraft(trimmed, description.trim()) },
              });
              setName("");
              setDescription("");
              onOpenChange(false);
            }}
          >
            Open canvas
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
