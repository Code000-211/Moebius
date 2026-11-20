/**
 * The rules of one rules file as 36px rows: the rule's name in mono, its text cut to one line, and at the right how
 * often it refused this week. A row opens the rule in the editor window (`EditorWindow`) with its name and text, Save,
 * Cancel and Delete (which asks first); a last row, "Add a rule", opens the same window empty. A save puts the rule's
 * section back into the file's text (`file.ts`) and hands the whole file to `onSave`, which the server checks; a
 * refusal stays in the window, beside the buttons, and a save is confirmed by a toast.
 */

import { useState } from "react";
import { sayError } from "../../api/errorText";
import type { Rule } from "../../api/types/rules";
import { DeleteButton, EditorWindow } from "../../ui/components/EditorWindow";
import { Input } from "../../ui/components/Input";
import { AddIcon } from "../../ui/icons";
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

  return (
    <div className="flex flex-col">
      {rules.length === 0 && <p className="m-0 px-4 py-2 text-base text-muted">{empty}</p>}
      {rules.map((rule) => (
        <RuleRow
          key={rule.name}
          name={rule.name}
          text={rule.text}
          state={refused(refusals, rule.name)}
          onOpen={editable ? () => open(rule) : undefined}
        />
      ))}
      {editable && (
        <RuleRow
          name="Add a rule"
          icon={<AddIcon className="size-4 shrink-0 text-muted" aria-hidden="true" />}
          onOpen={() => open(null)}
        />
      )}
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

/** One rule as a row of 36px; without `onOpen` it is read only. */
export function RuleRow({
  name,
  text,
  state,
  icon,
  onOpen,
}: {
  name: string;
  text?: string;
  /** What is said at the right: how often it refused, or that it is always on. */
  state?: string;
  icon?: React.ReactNode;
  onOpen?: () => void;
}) {
  const body = (
    <>
      {icon}
      <span
        className={cn("shrink-0 truncate", text ? "font-mono text-sm font-medium" : "text-muted")}
        style={text ? { maxWidth: "14rem" } : undefined}
      >
        {name}
      </span>
      {text && <span className="min-w-0 flex-1 truncate text-muted">{text}</span>}
      {state && (
        <span className="ml-auto shrink-0 text-xs tabular-nums text-attention-text">{state}</span>
      )}
    </>
  );
  const row =
    "flex h-9 w-full items-center gap-2.5 border-b border-hairline px-4 text-left text-base text-text";
  return onOpen ? (
    <button
      type="button"
      aria-label={text ? `Edit rule ${name}` : name}
      onClick={onOpen}
      className={cn(row, "cursor-pointer hover:bg-row-hover")}
    >
      {body}
    </button>
  ) : (
    <div className={row}>{body}</div>
  );
}
