/**
 * The page templates of the wiki, which live in the harness configuration repository (`/v1/templates`). "From a
 * template" sits above the editor of a new page: pick a template and a name, and the server makes the page from it
 * (`POST /v1/wiki/from-template`). "Templates" in the page's header opens where a member makes, edits and deletes them;
 * each save or delete is a commit of the configuration repository. A server without that repository lists none and
 * refuses an edit, which the dialog says.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  deletePageTemplate,
  listTemplates,
  pageFromTemplate,
  setPageTemplate,
} from "../../api/calls/templates";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { WikiPageView } from "../../api/types/wiki";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Input } from "../../ui/components/Input";
import { Field, Select, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { pathProblem } from "./model";

/** The templates, read once for the page and made stale by every change of them. */
export function useTemplates() {
  return useQuery({ queryKey: keys.templates, queryFn: listTemplates, retry: false });
}

/** A template's name: letters, digits, `-` and `_`, up to 64, as the server takes it. */
export const NAME_PATTERN = /^[A-Za-z0-9_-]{1,64}$/;

/** Makes a page at `folder`/name from a template chosen here. */
export function FromTemplate({
  folder,
  keys: projectKeys,
  onMade,
}: {
  folder: string;
  keys: readonly string[];
  onMade: (page: WikiPageView) => void;
}) {
  const templates = useTemplates();
  const names = (templates.data?.pages ?? []).map((template) => template.name);
  const [chosen, setChosen] = useState<string | null>(null);
  const [name, setName] = useState("");
  const template = chosen !== null && names.includes(chosen) ? chosen : (names[0] ?? "");
  const target = folder === "" ? name.trim() : `${folder}/${name.trim()}`;
  const problem = name.trim() === "" ? null : pathProblem(target, projectKeys);
  const make = useMutation({
    mutationFn: () => pageFromTemplate(template, target),
    onSuccess: onMade,
  });
  if (names.length === 0) return null;
  const ready = name.trim() !== "" && problem === null && !make.isPending;
  return (
    <form
      aria-label="Make a page from a template"
      className="flex min-w-0 flex-col gap-3 rounded-card bg-surface p-4 outline outline-1 -outline-offset-1 outline-hairline"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) make.mutate();
      }}
    >
      <h3 className="m-0 text-base font-semibold text-text">From a template</h3>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Template">
          <Select
            aria-label="Template"
            options={names}
            value={template}
            onChange={(next) => {
              make.reset();
              setChosen(next);
            }}
          />
        </Field>
        <Field
          label="Name of the page"
          hint={folder === "" ? "A slash makes a folder: Team/On call." : `In ${folder}/.`}
          error={problem ?? undefined}
        >
          <Input
            value={name}
            autoComplete="off"
            onChange={(event) => {
              make.reset();
              setName(event.target.value);
            }}
          />
        </Field>
      </div>
      {make.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't make the page. </span>
          {sayError(make.error, "Try again.")}
        </Notice>
      )}
      <div>
        <Button type="submit" size="sm" disabled={!ready}>
          {make.isPending ? "Making" : "Make page"}
        </Button>
      </div>
    </form>
  );
}

/** Where a member makes, edits and deletes the page templates. */
export function TemplatesDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const templates = useTemplates();
  // The template being edited: its name, or null for a new one.
  const [editing, setEditing] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.templates });
  const reset = () => {
    setEditing(null);
    setName("");
    setText("");
  };
  const save = useMutation({
    mutationFn: () => setPageTemplate({ name: name.trim(), text }),
    onSuccess: async () => {
      await refresh();
      reset();
    },
  });
  const remove = useMutation({
    mutationFn: (target: string) => deletePageTemplate(target),
    onSuccess: async (_, target) => {
      await refresh();
      if (editing === target) reset();
    },
  });
  const pages = templates.data?.pages ?? [];
  const nameOk = NAME_PATTERN.test(name.trim());
  const editable = templates.data?.editable ?? false;
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          reset();
          save.reset();
          remove.reset();
        }
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <div className="flex flex-col gap-4 p-6">
          <DialogHeader>
            <DialogTitle>Page templates</DialogTitle>
            <DialogDescription>
              A template is the text a new page starts with. Each change is a commit of the harness
              configuration repository.
            </DialogDescription>
          </DialogHeader>
          {templates.isError && (
            <Notice tone="error">
              {sayError(templates.error, "Could not read the templates.")}
            </Notice>
          )}
          {templates.isSuccess && !editable && (
            <Notice>
              This server has no harness configuration repository, so templates cannot be kept.
            </Notice>
          )}
          {pages.length === 0 && templates.isSuccess && editable && (
            <p className="m-0 text-sm text-secondary">No page template yet.</p>
          )}
          {pages.length > 0 && (
            // biome-ignore lint/a11y/noRedundantRoles: preflight takes the bullets off, and WebKit then drops the list
            <ul role="list" aria-label="Page templates" className="m-0 flex flex-col gap-1 p-0">
              {pages.map((template) => (
                <li
                  key={template.name}
                  className="flex min-h-8 items-center justify-between gap-2 rounded-control px-3 hover:bg-hover"
                >
                  <span className="min-w-0 truncate text-base text-text">{template.name}</span>
                  {editable && (
                    <span className="flex gap-1">
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label={`Edit the template ${template.name}`}
                        onClick={() => {
                          save.reset();
                          setEditing(template.name);
                          setName(template.name);
                          setText(template.text);
                        }}
                      >
                        Edit
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        aria-label={`Delete the template ${template.name}`}
                        disabled={remove.isPending}
                        onClick={() => remove.mutate(template.name)}
                      >
                        Delete
                      </Button>
                    </span>
                  )}
                </li>
              ))}
            </ul>
          )}
          {remove.isError && (
            <Notice tone="error">{sayError(remove.error, "Could not delete the template.")}</Notice>
          )}
          {editable && (
            <form
              aria-label={editing === null ? "New page template" : `Edit ${editing}`}
              className="flex flex-col gap-3"
              onSubmit={(event) => {
                event.preventDefault();
                if (nameOk && !save.isPending) save.mutate();
              }}
            >
              <Field
                label="Template name"
                hint="Letters, digits, - and _."
                error={
                  name.trim() !== "" && !nameOk ? "Use letters, digits, - and _ only." : undefined
                }
              >
                <Input
                  value={name}
                  disabled={editing !== null}
                  autoComplete="off"
                  onChange={(event) => setName(event.target.value)}
                />
              </Field>
              <Field label="Template text">
                <TextArea
                  mono
                  rows={8}
                  value={text}
                  onChange={(event) => setText(event.target.value)}
                />
              </Field>
              {save.isError && (
                <Notice tone="error">
                  <span className="font-semibold">Couldn't save the template. </span>
                  {sayError(save.error, "Try again.")}
                </Notice>
              )}
              <div className="flex gap-2">
                <Button type="submit" size="sm" disabled={!nameOk || save.isPending}>
                  {save.isPending ? "Saving" : editing === null ? "Add template" : "Save template"}
                </Button>
                {editing !== null && (
                  <Button type="button" size="sm" variant="secondary" onClick={reset}>
                    New template
                  </Button>
                )}
              </div>
            </form>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
