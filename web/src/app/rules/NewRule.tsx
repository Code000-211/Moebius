/**
 * The window "New rule" opens: it asks for the scope the rule belongs to, the server or one repository, starting on
 * the one the page was reading. Continue leaves the window and opens the editor on that scope's page.
 */
import { useState } from "react";
import type { RulesView } from "../../api/types/rules";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Field, Select } from "../../ui/page/Field";
import { scopeKey } from "./scopes";

export function NewRule({
  open,
  onOpenChange,
  view,
  initial,
  onChoose,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  view: RulesView;
  /** The scope it starts on, as `scopeKey` says it. */
  initial: string;
  onChoose: (scope: string) => void;
}) {
  const options: Array<[string, string]> = [
    ...(view.prose.editable ? [["server", "Server"] as [string, string]] : []),
    ...view.repositories.map((each): [string, string] => [
      scopeKey({ kind: "repository", id: each.repository_id }),
      each.repository,
    ]),
  ];
  const [picked, setPicked] = useState<string | null>(null);
  const value = picked ?? initial;
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setPicked(null);
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            setPicked(null);
            onChoose(value);
          }}
        >
          <DialogHeader>
            <DialogTitle>New rule</DialogTitle>
          </DialogHeader>
          <Field label="Scope">
            <Select aria-label="Scope" options={options} value={value} onChange={setPicked} />
          </Field>
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit">Continue</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
