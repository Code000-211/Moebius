/**
 * The editor of a wiki page: the Markdown at the left and how it reads at the right, side by side on a wide screen,
 * with an optional line for the commit. A new page also takes its path, in the folder it was started in. Saving is
 * `PUT /v1/wiki/pages/{path}`, a commit by the person pushed to the wiki's origin; when the push fails the server says
 * why and nothing is lost from the editor.
 */
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { writeWikiPage } from "../../api/calls/wiki";
import { sayError } from "../../api/errorText";
import type { WikiPageView } from "../../api/types/wiki";
import { Markdown } from "../../ui/chat/Markdown";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { pathProblem } from "./model";

export function PageEditor({
  path,
  folder,
  text,
  keys,
  onSaved,
  onCancel,
}: {
  /** The page edited; null for a new one. */
  path: string | null;
  /** Where a new page goes, empty at the top. */
  folder: string;
  text: string;
  /** The project keys a page may be under. */
  keys: readonly string[];
  onSaved: (page: WikiPageView) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [draft, setDraft] = useState(text);
  const [message, setMessage] = useState("");
  const target = path ?? (folder === "" ? name.trim() : `${folder}/${name.trim()}`);
  const problem = path === null ? pathProblem(target, keys) : null;
  const save = useMutation({
    mutationFn: () => writeWikiPage(target, draft, message.trim() || undefined),
    onSuccess: onSaved,
  });
  const ready = problem === null && !save.isPending && (path !== null || name.trim() !== "");
  return (
    <form
      aria-label={path === null ? "New page" : `Edit ${path}`}
      className="flex min-w-0 flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) save.mutate();
      }}
    >
      {path === null && (
        <Field
          label="Name"
          hint={folder === "" ? "A slash makes a folder: Team/On call." : `In ${folder}/.`}
          error={name.trim() !== "" && problem ? problem : undefined}
        >
          <Input
            value={name}
            placeholder="On call"
            autoComplete="off"
            onChange={(event) => setName(event.target.value)}
          />
        </Field>
      )}
      <div className="grid min-w-0 gap-4 xl:grid-cols-2">
        <Field label="Text">
          <TextArea
            rows={22}
            value={draft}
            mono
            placeholder={"# On call\n\nWho answers at night."}
            onChange={(event) => setDraft(event.target.value)}
          />
        </Field>
        <section aria-label="Preview" className="flex min-w-0 flex-col gap-1.5">
          <span className="text-sm font-semibold text-secondary">Preview</span>
          <div className="min-h-24 rounded-card bg-sunken p-4">
            {draft.trim() === "" ? (
              <p className="m-0 text-base text-secondary">Nothing written yet.</p>
            ) : (
              <Markdown content={draft} className="max-w-prose" />
            )}
          </div>
        </section>
      </div>
      <Field
        label="What changed"
        hint="The commit's message; without one it says the page was updated."
      >
        <Input
          value={message}
          autoComplete="off"
          onChange={(event) => setMessage(event.target.value)}
        />
      </Field>
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save the page. </span>
          {sayError(save.error, "Try again.")}
        </Notice>
      )}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={!ready}>
          {save.isPending ? "Saving" : "Save"}
        </Button>
        <Button type="button" variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
