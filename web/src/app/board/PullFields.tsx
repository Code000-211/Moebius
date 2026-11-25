/**
 * What a pull request's body says, read by the repository's pull request template (ADR 0034), in the card window: each
 * section under its heading, the tests as a checklist, the measurement as a state, and what the body lacks. Shown for the
 * card's own change and for a pull request the card follows; nothing until the body was read.
 */

import { CheckCircleIcon, CircleIcon } from "@phosphor-icons/react";
import type { CardChange, PullFields as Fields } from "../../api/types/board";

/** The heading a slug stands for: `what_the_change_does` is "What the change does". */
export function headingOf(slug: string): string {
  const words = slug.split("_").join(" ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

const TESTS = "tests_i_ran";
const MEASUREMENT = "measurement";

export function PullFields({ change }: { change: CardChange | null | undefined }) {
  const pull: Fields | undefined = change?.pull;
  if (!change || !pull) return null;
  const sections = Object.entries(pull.fields).filter(
    ([slug]) => slug !== TESTS && slug !== MEASUREMENT,
  );
  return (
    <section className="flex min-w-0 flex-col gap-3 px-6 pb-4" data-testid="pull-fields">
      <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
        <h3 className="m-0 text-sm font-semibold text-text">Pull request</h3>
        {change.draft && (
          <span className="text-sm text-secondary" data-testid="pull-draft">
            Draft
          </span>
        )}
        {pull.fixes.length > 0 && (
          <span className="text-sm text-muted">{`Fixes ${pull.fixes.map((n) => `#${n}`).join(", ")}`}</span>
        )}
      </div>
      {pull.missing.length > 0 && (
        <p className="m-0 text-sm text-danger" data-testid="pull-missing">
          {`The body lacks: ${pull.missing.join(", ")}.`}
        </p>
      )}
      <dl className="m-0 flex flex-col gap-2">
        {sections.map(([slug, text]) => (
          <div key={slug} className="flex min-w-0 flex-col gap-0.5">
            <dt className="text-sm font-medium text-secondary">{headingOf(slug)}</dt>
            <dd className="m-0 whitespace-pre-wrap text-sm text-text">{text}</dd>
          </div>
        ))}
        {pull.tests.length > 0 && (
          <div className="flex min-w-0 flex-col gap-0.5">
            <dt className="text-sm font-medium text-secondary">Tests I ran</dt>
            <dd className="m-0">
              <ul className="m-0 flex list-none flex-col gap-0.5 p-0" data-testid="pull-tests">
                {pull.tests.map((t) => (
                  <li key={t.text} className="flex items-start gap-1.5 text-sm text-text">
                    {t.done ? (
                      <CheckCircleIcon aria-label="done" className="mt-0.5 shrink-0" />
                    ) : (
                      <CircleIcon aria-label="not done" className="mt-0.5 shrink-0" />
                    )}
                    <span className="min-w-0">{t.text}</span>
                  </li>
                ))}
              </ul>
            </dd>
          </div>
        )}
        {pull.measurement && (
          <div className="flex min-w-0 flex-col gap-0.5">
            <dt className="text-sm font-medium text-secondary">Measurement</dt>
            <dd className="m-0 text-sm text-text" data-testid="pull-measurement">
              {pull.measurement.reason
                ? `${pull.measurement.state}: ${pull.measurement.reason}`
                : pull.measurement.state}
            </dd>
          </div>
        )}
      </dl>
    </section>
  );
}
