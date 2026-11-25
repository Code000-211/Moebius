/**
 * The daily digest (ADR 0031) an inbox item of the kind `daily_digest` holds, drawn in the window the Inbox opens for it:
 * per repository what merged and who wrote it (marked when a measurement came with it), the merges that were refused,
 * reverts, review rounds, who took or released an issue, what the stale handling did and what is held, then the tokens
 * against the day's budget, and what needs a person grouped by reason, each with the card it is about. A list the server
 * cut says how many more there were. It is short on purpose: nothing here is the owner's to do except the last part.
 */
import type { ReactNode } from "react";
import type { InboxItem } from "../../api/types/board";
import type { Digest, DigestLine, DigestSection } from "../../api/types/factory";
import { Button } from "../../ui/components/Button";
import { useOpenCard } from "../board/CardWindowHost";

/** The digest an item holds, or null for an item whose data is not one. */
export function digestOf(item: InboxItem): Digest | null {
  const data = item.data as Partial<Digest>;
  return Array.isArray(data.repositories) && Array.isArray(data.needs_a_person)
    ? (data as Digest)
    : null;
}

/** `1,234` for 1234, so that a figure of tokens reads at a glance. */
function figure(n: number): string {
  return n.toLocaleString("en-US");
}

/** The sections of a repository, in the order the digest tells them: the heading and its lines. */
function groups(section: DigestSection): { name: string; lines: DigestLine[]; more: number }[] {
  const rounds: DigestLine[] = [...section.rounds];
  if (section.review_runs > 0)
    rounds.push({
      number: null,
      title: `${section.review_runs} ${section.review_runs === 1 ? "review run" : "review runs"} of the Reviewer on its own changes`,
      who: null,
      detail: "",
      measured: false,
    });
  return [
    { name: "Merged", lines: section.merged, more: section.more.merged ?? 0 },
    { name: "Merge refused or failed", lines: section.refused, more: section.more.refused ?? 0 },
    { name: "Reverted", lines: section.reverts, more: section.more.reverts ?? 0 },
    { name: "Review rounds", lines: rounds, more: section.more.rounds ?? 0 },
    { name: "Taken", lines: section.taken, more: section.more.taken ?? 0 },
    { name: "Released", lines: section.released, more: section.more.released ?? 0 },
    { name: "Stale handling", lines: section.upkeep, more: section.more.upkeep ?? 0 },
    { name: "Held", lines: section.frozen, more: section.more.frozen ?? 0 },
  ].filter((group) => group.lines.length > 0);
}

function Line({ line, kind }: { line: DigestLine; kind: string }) {
  const parts: ReactNode[] = [];
  if (line.number !== null)
    parts.push(<span className="font-mono text-muted">#{line.number}</span>);
  if (line.title !== "") parts.push(<span className="text-text">{line.title}</span>);
  const who =
    line.who === null
      ? null
      : kind === "Merged"
        ? `by ${line.who}`
        : kind === "Held"
          ? `held by ${line.who}`
          : `@${line.who}`;
  return (
    <li className="flex flex-wrap items-baseline gap-x-2 text-base text-secondary">
      {parts.map((part, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: at most two parts, in a fixed order
        <span key={i}>{part}</span>
      ))}
      {who !== null && <span>{who}</span>}
      {line.measured && (
        <span
          className="rounded-control px-1.5 text-xs text-complete-text outline outline-1 -outline-offset-1 outline-hairline-strong"
          title="A measurement came with this change"
        >
          measured
        </span>
      )}
      {line.detail !== "" && <span className="text-muted">{line.detail}</span>}
    </li>
  );
}

export function DigestView({ digest }: { digest: Digest }) {
  const openCard = useOpenCard();
  const needs = digest.needs_a_person.reduce((sum, group) => sum + group.items.length, 0);
  const { spent, day_budget: budget } = digest.tokens;
  return (
    <div data-testid="digest" className="flex flex-col gap-5">
      {digest.repositories.length === 0 && (
        <p className="m-0 text-base text-secondary" data-testid="digest-quiet">
          Nothing happened in any repository since the last digest.
        </p>
      )}
      {digest.repositories.map((section) => (
        <section
          key={section.id}
          data-testid="digest-repository"
          aria-label={section.name}
          className="flex flex-col gap-3 rounded-control bg-sunken p-4"
        >
          <h3 className="m-0 text-md font-semibold text-text">{section.name}</h3>
          {groups(section).map((group) => (
            <div key={group.name} className="flex flex-col gap-1">
              <h4 className="m-0 text-sm font-semibold text-secondary">{group.name}</h4>
              <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
                {group.lines.map((line, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: lines have no id of their own
                  <Line key={i} line={line} kind={group.name} />
                ))}
              </ul>
              {group.more > 0 && <p className="m-0 text-sm text-muted">and {group.more} more</p>}
            </div>
          ))}
        </section>
      ))}
      <p className="m-0 text-base text-secondary" data-testid="digest-tokens">
        <span className="font-semibold text-text">Tokens</span> {figure(spent)} since the last
        digest
        {budget !== null ? `, against a budget of ${figure(budget)} a day` : ""}.
      </p>
      <section
        data-testid="digest-needs"
        aria-label="Needs a person"
        className="flex flex-col gap-3"
      >
        <h3 className="m-0 text-md font-semibold text-text">
          {needs === 0 ? "Nothing needs a person" : `Needs a person (${needs})`}
        </h3>
        {digest.needs_a_person.map((group) => (
          <div key={group.kind} className="flex flex-col gap-1" data-kind={group.kind}>
            <h4 className="m-0 text-sm font-semibold text-secondary">
              {group.reason} ({group.items.length})
            </h4>
            <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
              {group.items.map((each) => (
                <li key={each.id} className="flex flex-wrap items-baseline gap-x-2 text-base">
                  {each.card_id !== null ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="min-h-0 whitespace-normal px-1 text-left text-accent"
                      onClick={() => openCard(each.card_id as number)}
                    >
                      {each.number !== null ? `#${each.number} ` : ""}
                      {each.title}
                    </Button>
                  ) : (
                    <span className="text-text">{each.title}</span>
                  )}
                  {each.repository !== null && (
                    <span className="text-sm text-muted">{each.repository}</span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </section>
    </div>
  );
}
