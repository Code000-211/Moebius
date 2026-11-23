/**
 * A page's labels, in the head of the page: "Runbook · On call · + Label". A member removes one with the cross on it or
 * adds one with "+ Label", which opens a small field that offers the labels in use; each change is a commit of the
 * page's front matter (`PUT /v1/wiki/labels/{path}`) built on the version the page was read at, so a page someone saved
 * meanwhile is refused, not overwritten. A viewer, an older version and a page being edited show the labels without the
 * controls.
 */
import { PlusIcon, XIcon } from "@phosphor-icons/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { setWikiLabels, staleWrite } from "../../api/calls/wiki";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { WikiPageView } from "../../api/types/wiki";
import { useToast } from "../../ui/page/Toast";
import { labelProblem } from "./model";

const CHIP =
  "inline-flex min-h-6 items-center gap-1 rounded-control bg-sunken px-2 text-sm font-medium text-secondary";

export function PageLabels({
  page,
  editable,
  known,
}: {
  page: WikiPageView;
  /** Whether the person may change them: a member, on the page as it is, not while it is edited. */
  editable: boolean;
  /** The labels in use, offered when one is added. */
  known: readonly string[];
}) {
  const queryClient = useQueryClient();
  const toast = useToast();
  const list = useId();
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const set = useMutation({
    mutationFn: (labels: string[]) => setWikiLabels(page.path, labels, page.version.commit),
    onSuccess: () => {
      setAdding(false);
      setDraft("");
      return queryClient.invalidateQueries({ queryKey: keys.wiki });
    },
    onError: (error) => {
      toast({
        kind: "error",
        text: staleWrite(error)
          ? "Someone saved this page a moment ago. Its labels are as they are now; try again."
          : `Couldn't change the labels. ${sayError(error, "Try again.")}`,
      });
      void queryClient.invalidateQueries({ queryKey: keys.wiki });
    },
  });
  const text = draft.trim();
  const problem = text === "" ? null : labelProblem(text);
  const add = () => {
    if (text === "" || problem !== null || set.isPending) return;
    if (page.labels.includes(text)) {
      setAdding(false);
      setDraft("");
      return;
    }
    set.mutate([...page.labels, text]);
  };
  if (page.labels.length === 0 && !editable) return null;
  return (
    <fieldset
      aria-label="Labels"
      className="m-0 flex min-w-0 flex-wrap items-center gap-1.5 border-0 p-0"
    >
      {page.labels.map((label) => (
        <span key={label} className={CHIP}>
          {label}
          {editable && (
            <button
              type="button"
              aria-label={`Remove the label ${label}`}
              disabled={set.isPending}
              onClick={() => set.mutate(page.labels.filter((each) => each !== label))}
              className="inline-flex size-4 cursor-pointer items-center justify-center rounded-control bg-transparent text-muted hover:bg-hover hover:text-text"
            >
              <XIcon className="size-3" aria-hidden />
            </button>
          )}
        </span>
      ))}
      {editable &&
        (adding ? (
          <form
            className="flex items-center gap-1.5"
            onSubmit={(event) => {
              event.preventDefault();
              add();
            }}
          >
            <input
              // biome-ignore lint/a11y/noAutofocus: the field opens because the person asked for it
              autoFocus
              aria-label="New label"
              aria-invalid={problem !== null}
              list={list}
              value={draft}
              autoComplete="off"
              maxLength={40}
              placeholder="Label"
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  setAdding(false);
                  setDraft("");
                }
              }}
              className="h-6 w-32 rounded-control bg-field px-2 text-sm text-text outline outline-1 -outline-offset-1 outline-hairline-strong focus:outline-2 focus:outline-accent"
            />
            <datalist id={list}>
              {known
                .filter((label) => !page.labels.includes(label))
                .map((label) => (
                  <option key={label} value={label} />
                ))}
            </datalist>
            {problem !== null && <span className="text-xs text-error-text">{problem}</span>}
          </form>
        ) : (
          <button
            type="button"
            onClick={() => setAdding(true)}
            className="inline-flex min-h-6 cursor-pointer items-center gap-1 rounded-control bg-transparent px-1.5 text-sm text-muted transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
          >
            <PlusIcon className="size-3.5" aria-hidden />
            Label
          </button>
        ))}
    </fieldset>
  );
}
