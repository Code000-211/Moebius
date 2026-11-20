/**
 * The issue template of a kind, in the kind's box on the Kinds page: the title and description a new issue of the kind
 * starts with, and a value for any of the kind's fields (`PUT /v1/templates/issues/{kind}`). A kind with no template
 * says so and offers to make one; a saved one can be changed or deleted. Templates are commits of the harness
 * configuration repository, so a server without one says that it cannot keep them.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteIssueTemplate, listTemplates, setIssueTemplate } from "../../api/calls/templates";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { FieldValue, Kind } from "../../api/types/kinds";
import type { IssueTemplate } from "../../api/types/templates";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Field, Select, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";

/** The template as the form holds it: every value of a field is text until it is sent. */
interface Draft {
  title: string;
  body: string;
  fields: Record<string, string>;
}

function draftOf(template?: IssueTemplate): Draft {
  return {
    title: template?.title ?? "",
    body: template?.body ?? "",
    fields: Object.fromEntries(
      Object.entries(template?.fields ?? {}).map(([name, value]) => [name, String(value)]),
    ),
  };
}

/** The values of the fields that are filled, a number field's as a number. */
export function fieldValues(kind: Kind, draft: Draft): Record<string, FieldValue> {
  const values: Record<string, FieldValue> = {};
  for (const field of kind.fields) {
    const text = (draft.fields[field.name] ?? "").trim();
    if (text === "") continue;
    values[field.name] =
      field.type === "number" && !Number.isNaN(Number(text)) ? Number(text) : text;
  }
  return values;
}

export function IssueTemplateBox({ kind }: { kind: Kind }) {
  const templates = useQuery({ queryKey: keys.templates, queryFn: listTemplates, retry: false });
  if (templates.isPending || templates.isError) {
    return templates.isError ? (
      <Notice tone="error">{sayError(templates.error, "Could not read the templates.")}</Notice>
    ) : null;
  }
  const template = templates.data.issues.find(
    (t) => t.kind.toLowerCase() === kind.name.toLowerCase(),
  );
  return (
    <TemplateForm
      // A saved template read again resets the form to what the server holds.
      key={JSON.stringify([kind.fields.map((f) => f.name), template ?? null])}
      kind={kind}
      template={template}
      editable={templates.data.editable}
    />
  );
}

function TemplateForm({
  kind,
  template,
  editable,
}: {
  kind: Kind;
  template?: IssueTemplate;
  editable: boolean;
}) {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<Draft>(() => draftOf(template));
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.templates });
  const save = useMutation({
    mutationFn: () =>
      setIssueTemplate(kind.name, {
        title: draft.title.trim(),
        body: draft.body,
        fields: fieldValues(kind, draft),
      }),
    onSuccess: refresh,
  });
  const remove = useMutation({
    mutationFn: () => deleteIssueTemplate(kind.name),
    onSuccess: refresh,
  });
  const empty =
    draft.title.trim() === "" &&
    draft.body.trim() === "" &&
    Object.keys(fieldValues(kind, draft)).length === 0;
  return (
    <form
      aria-label={`Issue template of ${kind.name}`}
      className="flex flex-col gap-3 border-0 border-t border-solid border-hairline pt-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (!empty && editable && !save.isPending) save.mutate();
      }}
    >
      <h3 className="m-0 text-sm font-semibold text-secondary">
        Issue template {template ? "" : "(none yet)"}
      </h3>
      <p className="m-0 max-w-prose text-sm text-secondary">
        What a new issue of this kind starts with, when "Use the template" is ticked in New issue.
      </p>
      {!editable && (
        <Notice>
          This server has no harness configuration repository, so templates cannot be kept.
        </Notice>
      )}
      <Field label="Template title">
        <Input
          value={draft.title}
          disabled={!editable}
          onChange={(event) => setDraft({ ...draft, title: event.target.value })}
        />
      </Field>
      <Field label="Template description">
        <TextArea
          rows={4}
          value={draft.body}
          disabled={!editable}
          onChange={(event) => setDraft({ ...draft, body: event.target.value })}
        />
      </Field>
      {kind.fields.map((field) => {
        const set = (value: string) =>
          setDraft({ ...draft, fields: { ...draft.fields, [field.name]: value } });
        const value = draft.fields[field.name] ?? "";
        return field.type === "choice" ? (
          <Field key={field.id} label={`Template ${field.name}`}>
            <Select
              aria-label={`Template ${field.name}`}
              disabled={!editable}
              options={[["", "None"], ...field.options.map((o): [string, string] => [o, o])]}
              value={value}
              onChange={set}
            />
          </Field>
        ) : (
          <Field key={field.id} label={`Template ${field.name}`}>
            <Input
              value={value}
              disabled={!editable}
              placeholder={field.type === "date" ? "YYYY-MM-DD" : undefined}
              onChange={(event) => set(event.target.value)}
            />
          </Field>
        );
      })}
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save the template. </span>
          {sayError(save.error, "Try again.")}
        </Notice>
      )}
      {remove.isError && (
        <Notice tone="error">{sayError(remove.error, "Could not delete the template.")}</Notice>
      )}
      {editable && (
        <div className="flex gap-2">
          <Button type="submit" size="sm" disabled={empty || save.isPending}>
            {save.isPending ? "Saving" : "Save template"}
          </Button>
          {template && (
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={remove.isPending}
              onClick={() => remove.mutate()}
            >
              Delete template
            </Button>
          )}
        </div>
      )}
    </form>
  );
}
