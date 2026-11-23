/**
 * Where the wiki's pages are kept, as the Settings page shows it: on this server only, or in a git repository, with the
 * way to connect one (the same dialog as the Wiki page) while they are kept here only. A server that could not open
 * its wiki says why. Only an owner reaches the Settings page, so only an owner sees this.
 */
import { useState } from "react";
import { Button } from "../../ui/components/Button";
import { FieldRow, Rows } from "../../ui/page/Field";
import { ConnectWikiDialog, useWikiStatus } from "../wiki/WikiRepository";
import { Code, Section } from "./parts";

export function WikiBox() {
  const status = useWikiStatus();
  const [open, setOpen] = useState(false);
  if (!status) return null;
  const value =
    status.state === "remote" ? (
      <Code>{status.url ?? ""}</Code>
    ) : status.state === "local" ? (
      "Kept on this server only"
    ) : (
      `Not open: ${status.problem ?? "unknown reason"}`
    );
  return (
    <Section title="Wiki">
      <Rows>
        <FieldRow label="Wiki repository" wide>
          <span className="flex min-w-0 flex-wrap items-center justify-end gap-3">
            <span className="min-w-0 break-all text-right text-sm text-text">{value}</span>
            {status.state !== "remote" && (
              <Button size="sm" variant="secondary" onClick={() => setOpen(true)}>
                Use a git repository…
              </Button>
            )}
          </span>
        </FieldRow>
      </Rows>
      <ConnectWikiDialog open={open} onOpenChange={setOpen} />
    </Section>
  );
}
