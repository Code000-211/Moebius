/**
 * The Terminal page of the sidebar: a shell on the server, in the directory of `moebius.toml`
 * (`GET /v1/terminal`), for a person to look at the configuration and the machine Moebius runs on. It is dark in both
 * themes and takes the whole panel under the header. Closing this page closes the shell. Opening one is recorded by
 * the server as a judgement of kind `open_terminal`; what is typed or seen is not.
 */
import { Terminal } from "../../terminal/LazyTerminal";
import { PageHeader } from "../../ui/page/PageHeader";

export function TerminalPage() {
  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="terminal-page">
      <PageHeader title="Terminal" />
      <Terminal
        path="/v1/terminal"
        autoFocus
        variant="page"
        className="-mx-4 -mt-4 -mb-4 min-h-0 flex-1"
      />
    </div>
  );
}
