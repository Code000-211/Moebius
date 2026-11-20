/**
 * The dialog that starts a run of a workflow by hand. It reads the inputs the workflow's `manual` trigger names
 * (`GET /v1/workflows/{name}/run-inputs`) and shows one field for each, of the kind its type says, with the repository
 * the run belongs to when the trigger has no input of its own for it. A refusal of the server names the input it
 * refused and is shown in the dialog; a run that started opens its page.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { runInputs, startWorkflowRun } from "../../api/calls/workflows";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { RunInput } from "../../api/types/workflows";
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
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";

/** What a person typed for one input, as the value the server takes: a number, a boolean or the text. */
function valueFor(input: RunInput, typed: string | undefined): unknown {
  if (typed === undefined || typed === "") return undefined;
  if (input.type === "number") return Number(typed);
  if (input.type === "boolean") return typed === "true";
  return typed;
}

export function RunWorkflowDialog({
  name,
  open,
  onOpenChange,
}: {
  name: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const declared = useQuery({
    queryKey: [...keys.workflow(name), "run-inputs"],
    queryFn: () => runInputs(name),
    enabled: open,
    retry: false,
  });
  const [typed, setTyped] = useState<Record<string, string>>({});
  const [repository, setRepository] = useState("");
  const inputs = declared.data?.inputs ?? [];
  const namesRepository = inputs.some((input) => input.type === "repository");
  const start = useMutation({
    mutationFn: () => {
      const given: Record<string, unknown> = {};
      for (const input of inputs) {
        const value = valueFor(input, typed[input.name]);
        if (value !== undefined) given[input.name] = value;
      }
      return startWorkflowRun(name, {
        ...(namesRepository ? {} : { repository: repository.trim() }),
        inputs: given,
      });
    },
    onSuccess: async (started) => {
      await queryClient.invalidateQueries({ queryKey: keys.workflows });
      await queryClient.invalidateQueries({ queryKey: keys.workflowRuns(name) });
      onOpenChange(false);
      navigate(`/workflow-runs/${started.id}`);
    },
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form
          className="flex flex-col gap-4 p-6"
          onSubmit={(event) => {
            event.preventDefault();
            if (!start.isPending) start.mutate();
          }}
        >
          <DialogHeader>
            <DialogTitle>{`Run ${name}`}</DialogTitle>
            <DialogDescription>
              The run starts at once and opens on its page. A run can be cancelled there.
            </DialogDescription>
          </DialogHeader>
          {declared.isPending ? (
            <SkeletonLines lines={3} message="Reading the inputs" />
          ) : declared.isError ? (
            <ReadFailed
              error={declared.error}
              fallback="Could not read what this workflow asks for. Only a workflow started by hand can be run here."
              busy={declared.isFetching}
              onRetry={() => void declared.refetch()}
            />
          ) : (
            <>
              {namesRepository ? null : (
                <Field label="Repository" hint="The name of the repository the run belongs to.">
                  {(id) => (
                    <Input
                      id={id}
                      mono
                      autoFocus
                      value={repository}
                      onChange={(event) => setRepository(event.target.value)}
                    />
                  )}
                </Field>
              )}
              {inputs.map((input) => (
                <Field
                  key={input.name}
                  label={input.required ? `${input.name} (required)` : input.name}
                  hint={
                    input.default === undefined ? undefined : `Left empty: ${String(input.default)}`
                  }
                >
                  {(id) =>
                    input.type === "choice" || input.type === "boolean" ? (
                      <Select
                        id={id}
                        options={[
                          ["", "Not set"],
                          ...(input.type === "boolean"
                            ? ([
                                ["true", "Yes"],
                                ["false", "No"],
                              ] as Array<[string, string]>)
                            : (input.choices ?? []).map((choice): [string, string] => [
                                choice,
                                choice,
                              ])),
                        ]}
                        value={typed[input.name] ?? ""}
                        onChange={(value) => setTyped({ ...typed, [input.name]: value })}
                      />
                    ) : (
                      <Input
                        id={id}
                        mono={input.type === "repository"}
                        type={input.type === "number" ? "number" : "text"}
                        value={typed[input.name] ?? ""}
                        onChange={(event) =>
                          setTyped({ ...typed, [input.name]: event.target.value })
                        }
                      />
                    )
                  }
                </Field>
              ))}
            </>
          )}
          {start.isError ? (
            <Notice tone="error">
              {sayError(start.error, "Could not start the run. Try again.")}
            </Notice>
          ) : null}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button
              type="submit"
              variant="primary"
              disabled={start.isPending || !declared.isSuccess}
            >
              {start.isPending ? "Starting" : "Run"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
