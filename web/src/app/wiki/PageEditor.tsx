/**
 * The editor of a wiki page: the Markdown, and how it reads under it, with an optional line for the commit. A new
 * page also takes its path, in the folder it was started in. Saving is
 * `PUT /v1/wiki/pages/{path}`, a commit by the person pushed to the wiki's origin; when the push fails the server says
 * why and nothing is lost from the editor. The save carries `base`, the commit of the page as the editor loaded it, so
 * a page someone else saved meanwhile is refused (409) rather than overwritten: the editor says so (before saving too,
 * when `wiki_changed` shows a newer version), shows their version and lets the person keep their own text. Leaving with
 * unsaved text asks first (`LeaveGuard`). Typing `[[` offers the pages and the start of a card's name (`WEB-`) the
 * cards, under the text: arrows choose, Enter or Tab completes, Escape closes.
 */
import { useMutation, useQuery } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { snapshot } from "../../api/calls/board";
import { staleWrite, wikiPages, writeWikiPage } from "../../api/calls/wiki";
import { sayError } from "../../api/errorText";
import { keys as queryKeys } from "../../api/keys";
import type { WikiPageView } from "../../api/types/wiki";
import { Markdown } from "../../ui/chat/Markdown";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { useToast } from "../../ui/page/Toast";
import { cardKey } from "../board/cardKey";
import { LeaveGuard } from "./LeaveGuard";
import {
  type CardChoice,
  cardChoices,
  complete,
  completionAt,
  linkText,
  pageChoices,
} from "./links";
import { pathProblem } from "./model";

export function PageEditor({
  path,
  folder,
  text,
  version,
  latest,
  keys,
  onSaved,
  onCancel,
}: {
  /** The page edited; null for a new one. */
  path: string | null;
  /** Where a new page goes, empty at the top. */
  folder: string;
  text: string;
  /** The commit of the page as `text` is of it; null for a new page. */
  version: string | null;
  /** The page as it is now, which moves on while the editor is open; undefined for a new page. */
  latest?: WikiPageView;
  /** The project keys a page may be under. */
  keys: readonly string[];
  onSaved: (page: WikiPageView) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [draft, setDraft] = useState(text);
  const [loaded] = useState(text);
  // What the save is built on: the page as it was loaded, or as the person chose to keep their text over.
  const [base, setBase] = useState(version ?? "");
  const [refused, setRefused] = useState<{ current: WikiPageView | null } | null>(null);
  const [theirs, setTheirs] = useState(false);
  const saved = useRef(false);
  const now = useNow();
  const toast = useToast();
  const [message, setMessage] = useState("");
  const field = useRef<HTMLTextAreaElement>(null);
  const [caret, setCaret] = useState(0);
  const [active, setActive] = useState(0);
  const [closedAt, setClosedAt] = useState<number | null>(null);
  const pages = useQuery({ queryKey: queryKeys.wikiPages, queryFn: wikiPages, retry: false });
  const board = useQuery({ queryKey: queryKeys.snapshot, queryFn: snapshot });
  const cards: CardChoice[] = (board.data?.boards ?? []).flatMap((each) =>
    each.cards
      .filter((card) => card.project_key !== "")
      .map((card) => ({ key: cardKey(card), title: card.title })),
  );
  const completion = completionAt(draft, caret);
  const choices: Array<{ label: string; detail?: string; insert: string }> =
    completion === null || closedAt === completion.from
      ? []
      : completion.kind === "page"
        ? pageChoices(pages.data ?? [], completion.query).map((page) => ({
            label: page.name,
            detail: page.path === page.name ? undefined : page.path,
            insert: linkText(page, pages.data ?? []),
          }))
        : cardChoices(cards, completion.query).map((card) => ({
            label: card.key,
            detail: card.title,
            insert: card.key,
          }));
  const choose = (index: number) => {
    const picked = choices[index];
    if (!completion || !picked) return;
    const done = complete(draft, caret, completion, picked.insert);
    setDraft(done.text);
    setCaret(done.caret);
    setActive(0);
    requestAnimationFrame(() => {
      field.current?.focus();
      field.current?.setSelectionRange(done.caret, done.caret);
    });
  };
  const target = path ?? (folder === "" ? name.trim() : `${folder}/${name.trim()}`);
  const problem = path === null ? pathProblem(target, keys) : null;
  const save = useMutation({
    mutationFn: () => writeWikiPage(target, draft, message.trim() || undefined, base),
    onSuccess: (page) => {
      saved.current = true;
      onSaved(page);
    },
    onError: (error) => {
      const stale = staleWrite(error);
      setRefused(stale ?? null);
      if (stale === undefined)
        toast({ kind: "error", text: `Couldn't save the page. ${sayError(error, "Try again.")}` });
    },
  });
  // A page someone saved since it was loaded: found out by a refused save, or by the event that brought it.
  const newer = refused?.current ?? (latest && latest.version.commit !== base ? latest : null);
  const gone = refused !== null && refused.current === null;
  const dirty = () =>
    !saved.current && (path === null ? name.trim() !== "" || draft !== "" : draft !== loaded);
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
      <div className="grid min-w-0 gap-4">
        <div className="min-w-0">
          <Field label="Text">
            <TextArea
              ref={field}
              rows={14}
              value={draft}
              mono
              placeholder={"# On call\n\nWho answers at night."}
              onChange={(event) => {
                setDraft(event.target.value);
                setCaret(event.target.selectionStart);
                setActive(0);
              }}
              onSelect={(event) => setCaret(event.currentTarget.selectionStart)}
              onKeyDown={(event) => {
                if (choices.length === 0) return;
                if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                  event.preventDefault();
                  const step = event.key === "ArrowDown" ? 1 : -1;
                  setActive((active + step + choices.length) % choices.length);
                } else if (event.key === "Enter" || event.key === "Tab") {
                  event.preventDefault();
                  choose(active);
                } else if (event.key === "Escape") {
                  event.preventDefault();
                  setClosedAt(completion?.from ?? null);
                }
              }}
            />
          </Field>
          {choices.length > 0 && (
            <div
              role="listbox"
              aria-label={completion?.kind === "page" ? "Pages" : "Cards"}
              className="mt-1 flex flex-col rounded-card border border-hairline-strong bg-panel p-1"
            >
              {choices.map((choice, index) => (
                <button
                  type="button"
                  key={choice.insert}
                  role="option"
                  aria-selected={index === active}
                  tabIndex={-1}
                  onMouseDown={(event) => {
                    event.preventDefault();
                    choose(index);
                  }}
                  className="flex min-h-8 w-full cursor-pointer items-baseline bg-transparent text-left gap-2 rounded-control px-2 py-1 text-sm text-text aria-selected:bg-hover"
                >
                  <span className="font-semibold">{choice.label}</span>
                  {choice.detail && <span className="truncate text-muted">{choice.detail}</span>}
                </button>
              ))}
            </div>
          )}
        </div>
        <section
          aria-label="Preview"
          className="flex min-w-0 flex-col border-t border-hairline pt-4"
        >
          <div className="min-h-24">
            {draft.trim() === "" ? (
              <p className="m-0 text-base text-muted">Nothing written yet.</p>
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
      <LeaveGuard dirty={dirty} />
      {(newer !== null || gone) && (
        <Notice
          tone="error"
          action={
            <div className="flex flex-wrap gap-2">
              {newer !== null && (
                <Button
                  size="sm"
                  variant="secondary"
                  type="button"
                  aria-pressed={theirs}
                  onClick={() => setTheirs(!theirs)}
                >
                  {theirs ? "Hide their version" : "See their version"}
                </Button>
              )}
              <Button
                size="sm"
                variant="secondary"
                type="button"
                onClick={() => {
                  setBase(newer?.version.commit ?? "");
                  setRefused(null);
                  setTheirs(false);
                  save.reset();
                }}
              >
                {gone ? "Save it again" : "Keep my text"}
              </Button>
            </div>
          }
        >
          {gone ? (
            <>
              <span className="font-semibold">This page was deleted or moved. </span>
              Your text is still here.
            </>
          ) : (
            <>
              <span className="font-semibold">Someone saved a newer version. </span>
              {newer ? `${newer.version.author}, ${ago(now, newer.version.at)}. ` : ""}
              Your text is still here: copy what you need from theirs, or keep yours to replace it.
            </>
          )}
        </Notice>
      )}
      {theirs && newer !== null && (
        <section aria-label="Their version" className="flex min-w-0 flex-col gap-1.5">
          <span className="text-sm font-semibold text-secondary">Their version</span>
          <pre className="m-0 max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-card border border-hairline p-4 font-mono text-sm text-text">
            {newer.text}
          </pre>
        </section>
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
