/**
 * The dialog that starts a run of a project's workflow by hand ("Run a workflow here"). It reads the inputs the
 * workflow's `manual` trigger names (`GET /v1/projects/{id}/workflows/{name}/run-inputs`) and shows one field for each,
 * of the kind its type says. The repository the run belongs to is chosen among the project's own repositories, the
 * home one first, and is sent with the run; an input of type `repository` is a choice among the same repositories. A
 * project with no repository runs in its own files, so no repository is asked. When the dialog is opened from the tab
 * rather than from one workflow, it also asks which of the project's manual workflows to run. A refusal of the server
 * names the input it refused and is shown in the dialog; a run that started opens its page.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { projectRunInputs, startProjectWorkflowRun } from "../../api/calls/workflows";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import type { RunInput, WorkflowSummary } from "../../api/types/workflows";
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
import { workflowRunPath } from "../pages";

/** What a person typed for one input, as the value the server takes: a number, a boolean or the text. */
function valueFor(input: RunInput, typed: string | undefined): unknown {
  if (typed === undefined || typed === "") return undefined;
  if (input.type === "number") return Number(typed);
  if (input.type === "boolean") return typed === "true";
  return typed;
}

/** The repository a run starts in when the person chooses none: the project's home one, else its first. */
export function defaultRepository(project: Project): string {
  return (project.repositories.find((each) => each.home) ?? project.repositories[0])?.name ?? "";
}

export function RunWorkflowDialog({
  project,
  name,
  workflows,
  open,
  onOpenChange,
}: {
  project: Project;
  /** The workflow to run, when the dialog is opened from one; else it asks, among `workflows`. */
  name?: string;
  /** The workflows of the project the person may choose from, when `name` is not given. */
  workflows?: readonly WorkflowSummary[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const manual = (workflows ?? []).filter((each) => each.trigger_kind === "manual");
  const [picked, setPicked] = useState("");
  const chosen = name ?? (manual.some((each) => each.name === picked) ? picked : manual[0]?.name);
  const declared = useQuery({
    queryKey: [...keys.projectWorkflow(project.id, chosen ?? ""), "run-inputs"],
    queryFn: () => projectRunInputs(project.id, chosen ?? ""),
    enabled: open && chosen !== undefined,
    retry: false,
  });
  const [typed, setTyped] = useState<Record<string, string>>({});
  const [repository, setRepository] = useState("");
  const repositories = project.repositories.map((each): [string, string] => [each.name, each.name]);
  const where = repository || defaultRepository(project);
  const inputs = declared.data?.inputs ?? [];
  const namesRepository = inputs.some((input) => input.type === "repository");
  const start = useMutation({
    mutationFn: () => {
      const given: Record<string, unknown> = {};
      for (const input of inputs) {
        const value = valueFor(input, typed[input.name]);
        if (value !== undefined) given[input.name] = value;
      }
      return startProjectWorkflowRun(project.id, chosen ?? "", {
        ...(namesRepository || where === "" ? {} : { repository: where }),
        inputs: given,
      });
    },
    onSuccess: async (started) => {
      await queryClient.invalidateQueries({ queryKey: keys.workflows });
      await queryClient.invalidateQueries({ queryKey: keys.allWorkflowRuns });
      onOpenChange(false);
      navigate(workflowRunPath(started.id));
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
            <DialogTitle>
              {name === undefined ? `Run a workflow in ${project.name}` : `Run ${name}`}
            </DialogTitle>
            <DialogDescription>
              The run starts at once and opens on its page. A run can be cancelled there.
            </DialogDescription>
          </DialogHeader>
          {name === undefined && manual.length === 0 ? (
            <p className="m-0 text-sm text-secondary">
              This project has no workflow that is started by hand. Give a workflow a manual trigger
              on its canvas to run it here.
            </p>
          ) : declared.isPending ? (
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
              {name === undefined ? (
                <Field label="Workflow">
                  {(id) => (
                    <Select
                      id={id}
                      options={manual.map((each): [string, string] => [each.name, each.name])}
                      value={chosen ?? ""}
                      onChange={(value) => {
                        setPicked(value);
                        setTyped({});
                      }}
                    />
                  )}
                </Field>
              ) : null}
              {namesRepository || repositories.length === 0 ? null : (
                <Field label="Repository" hint="One of this project's repositories.">
                  {(id) => (
                    <Select id={id} options={repositories} value={where} onChange={setRepository} />
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
                    input.type === "choice" ||
                    input.type === "boolean" ||
                    input.type === "repository" ? (
                      <Select
                        id={id}
                        options={[
                          ["", "Not set"],
                          ...(input.type === "boolean"
                            ? ([
                                ["true", "Yes"],
                                ["false", "No"],
                              ] as Array<[string, string]>)
                            : input.type === "repository"
                              ? repositories
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
