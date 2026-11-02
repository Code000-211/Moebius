/**
 * The pieces the editors of the harness configuration share (Agents, Skills, MCP servers): the head of an editor with
 * the name of the item and its Delete and Save, the line that says a save became a commit, the row of those two buttons
 * on its own, and the drafts that keep what was typed while a person moves along the list. Everything else a page of
 * this kind needs — the box, the field, the notice, the chip, the select, the text area, the list beside its detail —
 * is in `src/ui/page`, which the pages import directly.
 */
import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";

/** The line that says an edit was committed, with the short id; or that it changed nothing. */
export function Committed({ commit }: { commit: string | null | undefined }) {
  return (
    <Notice tone="ok">
      {commit ? `Saved as commit ${commit}.` : "Saved. It changed nothing, so there is no commit."}
    </Notice>
  );
}

/**
 * The head of an editor: the name (an input while a new item is named), and Delete (which asks first) and Save. Save is
 * left out of `actions` for a read-only item. `dirty` says there is something the file does not have yet: without it
 * Save is disabled, and says nothing; `saveHint` is for the reason a person cannot see for themselves.
 */
export function EditorHead({
  name,
  naming,
  exists,
  deleteLabel,
  namePlaceholder,
  onName,
  saving,
  canSave,
  dirty = false,
  saveHint,
  onSave,
  onDelete,
  deleting,
  readOnly,
}: {
  name: string;
  naming: boolean;
  exists: boolean;
  deleteLabel: string;
  namePlaceholder: string;
  onName: (name: string) => void;
  saving: boolean;
  canSave: boolean;
  /** There is an edit that has not been saved. */
  dirty?: boolean;
  /** Why Save cannot be used, shown beside it. */
  saveHint?: string;
  onSave: () => void;
  onDelete: () => void;
  deleting: boolean;
  readOnly?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-4">
      {naming ? (
        <Field
          label={namePlaceholder}
          hint="Letters, digits, - and _."
          className="min-w-0 max-w-sm flex-1"
        >
          {(id) => (
            <Input
              id={id}
              mono
              autoFocus
              value={name}
              placeholder={namePlaceholder}
              onChange={(event) => onName(event.target.value)}
            />
          )}
        </Field>
      ) : (
        <h2 className="m-0 min-w-0 break-words text-lg font-semibold tracking-tight text-text">
          {name === "" ? "New item" : name}
        </h2>
      )}
      {readOnly ?? (
        <ActionRow
          exists={exists}
          deleteLabel={deleteLabel}
          name={name}
          saving={saving}
          canSave={canSave}
          dirty={dirty}
          saveHint={saveHint}
          onSave={onSave}
          onDelete={onDelete}
          deleting={deleting}
        />
      )}
    </div>
  );
}

/**
 * Delete (asking first) and Save, with what Save knows: work waiting, or why it cannot be used. There being nothing to
 * write is not said in words — a button that is quiet says that by being quiet — so Save is disabled instead.
 */
export function ActionRow({
  exists,
  deleteLabel,
  name,
  saving,
  canSave,
  dirty = false,
  saveHint,
  onSave,
  onDelete,
  deleting,
}: {
  exists: boolean;
  deleteLabel: string;
  name: string;
  saving: boolean;
  canSave: boolean;
  /** There is an edit that has not been saved. */
  dirty?: boolean;
  /** Why Save cannot be used, shown beside it instead of hidden in a tooltip. */
  saveHint?: string;
  onSave: () => void;
  onDelete: () => void;
  deleting: boolean;
}) {
  const [asking, setAsking] = useState(false);
  if (asking) {
    return (
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-base text-text">Delete {name}?</span>
        <Button variant="secondary" size="sm" onClick={() => setAsking(false)}>
          Keep
        </Button>
        <Button
          variant="danger"
          size="sm"
          disabled={deleting}
          onClick={() => {
            setAsking(false);
            onDelete();
          }}
        >
          {deleteLabel}
        </Button>
      </div>
    );
  }
  const state = saving
    ? "Saving"
    : !canSave
      ? (saveHint ?? "Cannot be saved")
      : dirty
        ? "Unsaved changes"
        : undefined;
  return (
    <div className="flex flex-wrap items-center justify-end gap-3">
      {exists && (
        <Button variant="danger" size="sm" onClick={() => setAsking(true)}>
          {deleteLabel}
        </Button>
      )}
      {state && <span className="text-sm text-secondary">{state}</span>}
      <Button size="sm" disabled={!canSave || saving || !dirty} onClick={onSave}>
        {saving ? "Saving" : "Save"}
      </Button>
    </div>
  );
}

/** The edits a person has typed but not saved, held by the name of the item they belong to. */
export type Drafts<T> = {
  /** What was typed for `name`, or `fresh` when nothing is being held for it. */
  take: (name: string, fresh: () => T) => T;
  /** What is in the editor for `name` now. */
  hold: (name: string) => (value: T) => void;
  /** Nothing is being held for `name` any more, because it was saved or deleted. */
  drop: (name: string) => void;
};

/**
 * The drafts of a list beside its editor, so that choosing another row does not throw away what was typed. The page
 * holds them, the editor reports each change, and a save or a delete clears the one it covers.
 */
export function useDrafts<T>(): Drafts<T> {
  const [held, setHeld] = useState<Record<string, T>>({});
  const take = useCallback(
    (name: string, fresh: () => T): T => {
      const value = held[name];
      return value === undefined ? fresh() : value;
    },
    [held],
  );
  const hold = useCallback(
    (name: string) => (value: T) => setHeld((before) => ({ ...before, [name]: value })),
    [],
  );
  const drop = useCallback(
    (name: string) =>
      setHeld((before) => {
        if (!(name in before)) return before;
        const next = { ...before };
        delete next[name];
        return next;
      }),
    [],
  );
  return { take, hold, drop };
}

/**
 * Keeps `value` with the page while it is work in progress, and forgets it once it is not, so that a draft never
 * outlives the change it belongs to. `drafts` is left out of the dependencies on purpose: the page hands over a new
 * function each render, and only the value matters here.
 */
export function useDraft<T>(
  value: T,
  dirty: boolean,
  drafts?: { hold: (value: T) => void; drop: () => void },
) {
  const where = useRef(drafts);
  where.current = drafts;
  useEffect(() => {
    const held = where.current;
    if (!held) return;
    if (dirty) held.hold(value);
    else held.drop();
  }, [value, dirty]);
}
