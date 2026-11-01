/**
 * The rules of one rules file as cards: each with its name, its text, what it refused this week and a control that
 * grows the card in place into an editor of the rule's name and text with Save, Cancel and Delete (which asks first).
 * Under the last rule an "Add a rule" control adds one. A save puts the rule's section back into the file's text
 * (`file.ts`) and hands the whole file to `onSave`, which the server checks; a refusal stays in the card, beside the
 * rule it refuses.
 */
import { PencilSimpleIcon } from "@phosphor-icons/react";
import { useState } from "react";
import { sayError } from "../../api/errorText";
import type { Rule } from "../../api/types/rules";
import { Tag, type TagTone } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { IconButton } from "../../ui/components/IconButton";
import { Input } from "../../ui/components/Input";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Body } from "../../ui/page/PageHeader";
import { appendRule, refusalsLabel, replaceRule, section, validName } from "./file";

/** A rule being edited: the one called `rule` of the file, or a new one. */
interface Editing {
  rule: string | null;
  name: string;
  text: string;
  deleting: boolean;
  saving: boolean;
  error: string | null;
}

/** The hue of a rule's refusals: a rule that refused is worth a look, one that did not is not. */
function refusalTone(refusals: Record<string, number>, name: string): TagTone {
  return (refusals[name] ?? 0) > 0 ? "attention" : "plain";
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
  const [editing, setEditing] = useState<Editing | null>(null);
  const [notice, setNotice] = useState<{ tone: "ok" | "error"; text: string } | null>(null);

  const edit = (rule: Rule | null) => {
    setNotice(null);
    setEditing({
      rule: rule?.name ?? null,
      name: rule?.name ?? "",
      text: rule?.text ?? "",
      deleting: false,
      saving: false,
      error: null,
    });
  };

  async function save(current: Editing, drop: boolean) {
    const edited = drop ? "" : section(current.name, current.text);
    const whole =
      current.rule === null ? appendRule(file, edited) : replaceRule(file, current.rule, edited);
    if (whole === null) {
      setEditing({ ...current, error: `The rule ${current.rule} is no longer in the file.` });
      return;
    }
    setEditing({ ...current, saving: true, error: null });
    try {
      const commit = await onSave(whole);
      setEditing(null);
      setNotice({ tone: "ok", text: commit ? `Committed: ${commit}` : "Saved." });
    } catch (error) {
      setEditing({
        ...current,
        saving: false,
        deleting: false,
        error: sayError(error, "Could not save the rules. Try again."),
      });
    }
  }

  return (
    <div className="flex flex-col gap-4">
      {rules.length === 0 && <Body>{empty}</Body>}
      {rules.map((rule) =>
        editing?.rule === rule.name ? (
          <RuleEditor
            key={rule.name}
            current={editing}
            onChange={setEditing}
            onSave={(drop) => void save(editing, drop)}
            onCancel={() => setEditing(null)}
          />
        ) : (
          <RuleCard
            key={rule.name}
            rule={rule}
            refusals={refusals}
            editable={editable}
            onEdit={() => edit(rule)}
          />
        ),
      )}
      {editable &&
        (editing && editing.rule === null ? (
          <RuleEditor
            current={editing}
            onChange={setEditing}
            onSave={(drop) => void save(editing, drop)}
            onCancel={() => setEditing(null)}
          />
        ) : (
          <Button size="sm" variant="secondary" onClick={() => edit(null)}>
            <PencilSimpleIcon className="size-4" aria-hidden="true" />
            Add a rule
          </Button>
        ))}
      {notice && <Notice tone={notice.tone}>{notice.text}</Notice>}
    </div>
  );
}

/** One rule as it stands: its name, what it says, how often it refused, and the control that opens it. */
function RuleCard({
  rule,
  refusals,
  editable,
  onEdit,
}: {
  rule: Rule;
  refusals: Record<string, number>;
  editable: boolean;
  onEdit: () => void;
}) {
  return (
    <article className="flex flex-col gap-4 rounded-card bg-sunken p-4">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="m-0 min-w-0 flex-1 break-words font-mono text-sm font-semibold text-text">
          {rule.name}
        </h3>
        <Tag tone={refusalTone(refusals, rule.name)} className="tabular-nums">
          {refusalsLabel(refusals, rule.name)}
        </Tag>
        {editable && (
          <IconButton
            icon={PencilSimpleIcon}
            aria-label={`Edit rule ${rule.name}`}
            onClick={onEdit}
          />
        )}
      </div>
      <p className="m-0 min-w-0 break-words text-base leading-normal text-text">{rule.text}</p>
    </article>
  );
}

/** A rule grown into its editor: its name and text, and what a save or a delete would do. */
function RuleEditor({
  current,
  onChange,
  onSave,
  onCancel,
}: {
  current: Editing;
  onChange: (next: Editing) => void;
  onSave: (drop: boolean) => void;
  onCancel: () => void;
}) {
  const busy = current.saving;
  return (
    <div className="flex flex-col gap-4 rounded-card bg-sunken p-4">
      <Field
        label="Rule name"
        hint="The name a refusal says. It cannot hold spaces."
        error={
          current.name.trim() !== "" && !validName(current.name)
            ? "Remove the spaces from the name."
            : undefined
        }
      >
        <Input
          placeholder="name-without-spaces"
          value={current.name}
          onChange={(event) => onChange({ ...current, name: event.target.value })}
          className="font-mono text-sm"
        />
      </Field>
      <Field label="Rule text">
        <TextArea
          placeholder="What a run must not do."
          rows={4}
          value={current.text}
          onChange={(event) => onChange({ ...current, text: event.target.value })}
        />
      </Field>
      {current.error && <Notice tone="error">{current.error}</Notice>}
      <div className="flex flex-wrap gap-2">
        <Button size="sm" disabled={busy || !validName(current.name)} onClick={() => onSave(false)}>
          {busy ? "Saving" : "Save"}
        </Button>
        <Button size="sm" variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
        {current.rule !== null && !current.deleting && (
          <Button
            size="sm"
            variant="secondary"
            onClick={() => onChange({ ...current, deleting: true })}
          >
            Delete rule
          </Button>
        )}
      </div>
      {current.deleting && (
        <div className="flex flex-col gap-4 rounded-control bg-surface p-4 sm:flex-row sm:items-center">
          <p className="m-0 flex min-h-10 min-w-0 flex-1 items-center break-words text-base text-text">
            Delete this rule?
          </p>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="secondary"
              onClick={() => onChange({ ...current, deleting: false })}
            >
              Keep it
            </Button>
            <Button size="sm" variant="danger" disabled={busy} onClick={() => onSave(true)}>
              Delete
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
