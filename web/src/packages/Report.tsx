/**
 * What the last request of the sheet told: the sentences of an install, update or removal, the changelog of an update
 * a person is about to merge, and the link to that change in the Inbox. A refusal is shown the same way, as a notice
 * in the sheet and never as a broken page.
 */
import { Link } from "react-router";
import { Markdown } from "../ui/chat/Markdown";
import { Button } from "../ui/components/Button";
import { Notice } from "../ui/page/Notice";
import { changeLink, type Told } from "./model";

/** What is shown after a request: its sentences, the changelogs that came with them, or the reason it was refused. */
export interface Report {
  told: Told[];
  logs: Array<{ name: string; text: string }>;
  error: string | null;
}

export const reported = (told: Told[], logs: Report["logs"] = []): Report => ({
  told,
  logs,
  error: null,
});

export const refused = (error: string): Report => ({ told: [], logs: [], error });

export function ReportView({ report, onLeave }: { report: Report; onLeave: () => void }) {
  return (
    <div className="flex flex-col gap-2" data-testid="package-report">
      {report.error !== null && <Notice tone="error">{report.error}</Notice>}
      {report.told.map((told) => (
        <Notice
          key={told.lines.join(" ")}
          tone={told.tone}
          action={
            told.change === null ? undefined : (
              <Button asChild size="sm" variant="secondary">
                <Link to={changeLink(told.change)} onClick={onLeave}>
                  See it in the Inbox
                </Link>
              </Button>
            )
          }
        >
          {told.lines.map((line) => (
            <span key={line} className="block">
              {line}
            </span>
          ))}
        </Notice>
      ))}
      {report.logs.map((log) => (
        <section
          key={log.name}
          aria-label={`Changelog of ${log.name}`}
          className="rounded-control bg-sunken p-3"
        >
          <h3 className="m-0 mb-2 text-sm font-semibold text-secondary">{`What changes in ${log.name}`}</h3>
          <Markdown content={log.text} />
        </section>
      ))}
    </div>
  );
}
