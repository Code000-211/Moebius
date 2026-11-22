/**
 * The rules of one rules file as blocks (`RuleBlock`): the rule's name and scope, its whole text as prose, and a muted
 * line of where it applies and how often it refused this week. A block opens the rule in the editor window
 * (`EditorWindow`) with its name and text, Save, Cancel and Delete (which asks first); a scope without a rule says so
 * and offers "Add a rule", which opens the same window empty, as does "New rule" of the page header (`creating`). A
 * save puts the rule's section back into the file's text (`file.ts`) and hands the whole file to `onSave`, which the
 * server checks; a refusal stays in the window, beside the buttons, and a save is confirmed by a toast.
 */

import { useEffect, useState } from "react";
import { sayError } from "../../api/errorText";
import type { Rule } from "../../api/types/rules";
import { Markdown } from "../../ui/chat/Markdown";
import { Button } from "../../ui/components/Button";
import { DeleteButton, EditorWindow } from "../../ui/components/EditorWindow";
import { Input } from "../../ui/components/Input";
import { LockIcon } from "../../ui/icons";
import { cn } from "../../ui/lib/cn";
import { plural } from "../../ui/lib/format";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Field, TextArea } from "../../ui/page/Field";
import { useToast } from "../../ui/page/Toast";
import { appendRule, replaceRule, section, validName } from "./file";

/** A rule being edited: the one called `rule` of the file, or a new one (`null`). */
interface Editing {
  rule: string | null;
  name: string;
  text: string;
}

/** What a row says on its right: how often the rule refused this week, and nothing when it did not. */
export function refused(refusals: Record<string, number>, name: string): string {
  const n = refusals[name] ?? 0;
  return n === 0 ? "" : plural(n, "refusal", "refusals");
}

export function RuleList({
  rules,
  file,
  refusals,
  editable,
  empty,
  chip,
  where,
  creating,
  onCreated,
  onSave,
}: {
  rules: Rule[];
  /** The text of the file the rules are in. */
  file: string;
  refusals: Record<string, number>;
  /** Whether the file can be saved; a server without a rules file shows its rules read only. */
  editable: boolean;
  /** What the list says when there is no rule. */
  empty: string;
  /** The scope each block names beside its rule. */
  chip: string;
  /** Where the rules apply, the first of each block's muted line. */
  where: string;
  /** True while the page asks for a new rule; `onCreated` is called once the empty editor is open. */
  creating: boolean;
  onCreated: () => void;
  /** Saves the whole file; resolves with the short id of the commit when the server says one. */
  onSave: (file: string) => Promise<string | undefined>;
}) {
  const toast = useToast();
  const [editing, setEditing] = useState<Editing | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  const open = (rule: Rule | null) => {
    setError(null);
    setEditing({ rule: rule?.name ?? null, name: rule?.name ?? "", text: rule?.text ?? "" });
  };
  const original = rules.find((each) => each.name === editing?.rule);
  const dirty =
    editing !== null &&
    (editing.rule === null
      ? editing.name !== "" || editing.text !== ""
      : editing.name !== original?.name || editing.text !== original?.text);

  /** Puts the edited section back (or drops it) and saves the file; throws what the server or the file refuses. */
  async function persist(current: Editing, drop: boolean) {
    const edited = drop ? "" : section(current.name, current.text);
    const whole =
      current.rule === null ? appendRule(file, edited) : replaceRule(file, current.rule, edited);
    if (whole === null) throw new Error(`The rule ${current.rule} is no longer in the file.`);
    const commit = await onSave(whole);
    toast({ kind: "confirmation", text: commit ? `Committed: ${commit}` : "Saved." });
  }

  async function save(current: Editing) {
    setSaving(true);
    setError(null);
    try {
      await persist(current, false);
      setEditing(null);
    } catch (failure) {
      setError(sayError(failure, "Could not save the rules. Try again."));
    }
    setSaving(false);
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: only a new request to create opens the editor.
  useEffect(() => {
    if (!creating) return;
    open(null);
    onCreated();
  }, [creating]);

  return (
    <div className="flex flex-col">
      {rules.length === 0 && (
        <div className="flex flex-col items-start gap-3">
          <p className="m-0 text-base text-muted">{empty}</p>
          {editable && (
            <Button size="sm" variant="secondary" onClick={() => open(null)}>
              Add a rule
            </Button>
          )}
        </div>
      )}
      {rules.map((rule) => (
        <RuleBlock
          key={rule.name}
          name={rule.name}
          text={rule.text}
          chip={chip}
          meta={[where, refused(refusals, rule.name)]}
          onOpen={editable ? () => open(rule) : undefined}
        />
      ))}
      {editing && (
        <EditorWindow
          open
          title={editing.rule === null ? "New rule" : `Edit rule ${editing.rule}`}
          dirty={dirty}
          saving={saving}
          canSave={validName(editing.name)}
          error={error}
          actions={
            editing.rule !== null && (
              <DeleteButton onClick={() => setDeleting(true)}>Delete rule</DeleteButton>
            )
          }
          onSave={() => void save(editing)}
          onClose={() => setEditing(null)}
        >
          <Field
            label="Rule name"
            hint="The name a refusal says. It cannot hold spaces."
            error={
              editing.name.trim() !== "" && !validName(editing.name)
                ? "Remove the spaces from the name."
                : undefined
            }
          >
            <Input
              placeholder="name-without-spaces"
              value={editing.name}
              onChange={(event) => setEditing({ ...editing, name: event.target.value })}
              className="font-mono text-sm"
            />
          </Field>
          <Field label="Rule text">
            <TextArea
              placeholder="What a run must not do."
              rows={5}
              value={editing.text}
              onChange={(event) => setEditing({ ...editing, text: event.target.value })}
            />
          </Field>
          <ConfirmDelete
            open={deleting}
            onOpenChange={setDeleting}
            title="Delete this rule?"
            confirmLabel="Delete rule"
            onConfirm={async () => {
              await persist(editing, true);
              setEditing(null);
            }}
          >
            The rule {editing.rule} is removed from the file.
          </ConfirmDelete>
        </EditorWindow>
      )}
    </div>
  );
}

/** The scope of a rule, a quiet tag after its name. */
export function ScopeChip({ children }: { children: string }) {
  return (
    <span className="shrink-0 rounded-control bg-sunken px-1.5 py-px text-xs text-secondary">
      {children}
    </span>
  );
}

/**
 * One rule as a block: its name (not mono) with its scope, the whole text wrapped as quiet prose, and a muted line of
 * what is known about it (empty parts are left out). Without `onOpen` it is read only and shows a lock; with it the
 * name and the block open the rule, and a link inside the text stays a link.
 */
export function RuleBlock({
  name,
  text,
  chip,
  meta,
  onOpen,
}: {
  name: string;
  text: string;
  chip: string;
  meta: string[];
  onOpen?: () => void;
}) {
  const said = meta.filter((part) => part !== "");
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the name inside is the keyboard's way to open the block.
    <article
      aria-label={name}
      onClick={
        onOpen
          ? (event) => {
              if (!(event.target as HTMLElement).closest("a, button")) onOpen();
            }
          : undefined
      }
      className={cn(
        "flex flex-col gap-1.5 border-b border-hairline py-4 first:pt-0",
        onOpen && "cursor-pointer",
      )}
    >
      <div className="flex min-w-0 items-center gap-2">
        {onOpen ? (
          <button
            type="button"
            aria-label={`Edit rule ${name}`}
            onClick={onOpen}
            className="m-0 min-w-0 cursor-pointer truncate bg-transparent p-0 text-left text-base font-semibold text-text hover:underline"
          >
            {name}
          </button>
        ) : (
          <h3 className="m-0 min-w-0 truncate text-base font-semibold text-text">{name}</h3>
        )}
        <ScopeChip>{chip}</ScopeChip>
        {!onOpen && (
          <LockIcon className="size-3.5 shrink-0 text-muted" aria-label="Read only" role="img" />
        )}
      </div>
      <Markdown content={text} className="text-secondary" />
      {said.length > 0 && <p className="m-0 text-sm text-muted">{said.join(" · ")}</p>}
    </article>
  );
}
