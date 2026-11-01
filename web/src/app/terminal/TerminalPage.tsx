/**
 * The Terminal page of the sidebar: a shell on the server, in the directory of `moebius.toml`
 * (`GET /v1/terminal`), for a person to look at the configuration and the machine Moebius runs on. Opening one is
 * recorded by the server as a judgement of kind `open_terminal`; what is typed or seen is not.
 */
import { Terminal } from "../../terminal/Terminal";
import { PageHeader } from "../../ui/page/PageHeader";

export function TerminalPage() {
  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="terminal-page">
      <PageHeader
        title="Terminal"
        context={
          <>
            A shell on the server, in the directory where{" "}
            <span className="font-mono text-sm">moebius.toml</span> is. Closing this page closes the
            shell.
          </>
        }
      />
      <Terminal path="/v1/terminal" autoFocus className="min-h-0 flex-1" />
    </div>
  );
}
