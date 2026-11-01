/**
 * The rules of one rules file as cards: each with its scope tag, name, text, what it refused and a pencil that grows
 * the card in place into an editor of the rule's name and text with Save, Cancel and Delete (which asks first). Under
 * the last rule an empty "Add a rule" card adds one. A save puts the rule's section back into the file's text
 * (`file.ts`) and hands the whole file to `onSave`, which the server checks; a refusal stays in the card.
 */
import { PencilSimpleIcon } from "@phosphor-icons/react";
import { useState } from "react";
import type { Rule } from "../../api/types/rules";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { cn } from "../../ui/lib/cn";
import { Notice, sayError, TextArea } from "../kit/Kit";
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

export function RuleList({
  scope,
  rules,
  file,
  refusals,
  editable,
  empty,
  onSave,
}: {
  /** The tag of every rule: "Server" or "Repo". */
  scope: "Server" | "Repo";
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
        error: sayError(error, "Could not save the rules."),
      });
    }
  }

  const card = (children: React.ReactNode, line: "plain" | "muted" | "editing", key: string) => (
    <div
      key={key}
      className={cn(
        "rounded-md border px-plusfifty py-base",
        line === "plain" && "bg-recessed",
        line === "muted" && "border-line-strong bg-lane",
        line === "editing" && "border-accent bg-card",
      )}
    >
      {children}
    </div>
  );

  const editor = (current: Editing) => (
    <div className="flex flex-col gap-plusfifty">
      <Input
        aria-label="Rule name"
        placeholder="name-without-spaces"
        value={current.name}
        className="font-mono"
        onChange={(event) => setEditing({ ...current, name: event.target.value })}
      />
      <TextArea
        aria-label="Rule text"
        placeholder="What a run must not do."
        rows={4}
        value={current.text}
        onChange={(event) => setEditing({ ...current, text: event.target.value })}
      />
      {current.error && <Notice tone="error">{current.error}</Notice>}
      <div className="flex flex-wrap items-center gap-base">
        <Button
          size="sm"
          disabled={current.saving || !validName(current.name)}
          onClick={() => void save(current, false)}
        >
          Save
        </Button>
        <Button size="sm" variant="outline" onClick={() => setEditing(null)}>
          Cancel
        </Button>
        {current.rule !== null &&
          (current.deleting ? (
            <span className="flex items-center gap-base text-sm text-normal">
              Delete this rule?
              <Button
                size="sm"
                variant="outline"
                onClick={() => setEditing({ ...current, deleting: false })}
              >
                Keep it
              </Button>
              <Button
                size="sm"
                variant="destructive"
                disabled={current.saving}
                onClick={() => void save(current, true)}
              >
                Delete
              </Button>
            </span>
          ) : (
            <Button
              size="sm"
              variant="outline"
              onClick={() => setEditing({ ...current, deleting: true })}
            >
              Delete rule
            </Button>
          ))}
      </div>
    </div>
  );

  const pencil = (label: string, onClick: () => void) => (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="rounded-sm p-half text-low hover:bg-card hover:text-high"
    >
      <PencilSimpleIcon className="size-icon-sm" />
    </button>
  );

  return (
    <div className="flex flex-col gap-base">
      {rules.length === 0 && !editable && <p className="text-sm text-normal">{empty}</p>}
      {rules.map((rule) =>
        editing?.rule === rule.name
          ? card(editor(editing), "editing", rule.name)
          : card(
              <div className="flex flex-wrap items-center gap-plusfifty">
                <Tag tone={scope === "Server" ? "amber" : "plain"}>{scope}</Tag>
                <span className="flex w-[210px] items-center gap-half font-mono text-sm text-high">
                  {rule.name}
                  {editable && pencil(`Edit rule ${rule.name}`, () => edit(rule))}
                </span>
                <span className="min-w-[200px] flex-1 break-words text-xs text-normal">
                  {rule.text}
                </span>
                <Tag tone={(refusals[rule.name] ?? 0) > 0 ? "red" : "plain"}>
                  {refusalsLabel(refusals, rule.name)}
                </Tag>
              </div>,
              "plain",
              rule.name,
            ),
      )}
      {editable &&
        (editing && editing.rule === null ? (
          card(editor(editing), "editing", "new")
        ) : (
          <div className="rounded-md border border-line-strong bg-lane px-plusfifty py-base">
            <span className="flex items-center gap-half text-sm text-low">
              {rules.length === 0 ? `${empty} Add a rule` : "Add a rule"}
              {pencil("Add a rule", () => edit(null))}
            </span>
          </div>
        ))}
      {notice && <Notice tone={notice.tone}>{notice.text}</Notice>}
    </div>
  );
}
