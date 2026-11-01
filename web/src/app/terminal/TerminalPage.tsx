/**
 * The Terminal page of the sidebar: a shell on the server, in the directory of `moebius.toml`
 * (`GET /v1/terminal`), for a person to look at the configuration and the machine Moebius runs on. Opening one is
 * recorded by the server as a judgement of kind `open_terminal`; what is typed or seen is not.
 */
import { Terminal } from "../../terminal/Terminal";
import { Header } from "../shell/Page";

export function TerminalPage() {
  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="terminal-page">
      <Header breadcrumb="Setup" title="Terminal" />
      <p className="mb-base mt-0 text-xs text-normal">
        A shell on the server, where <span className="font-mono">moebius.toml</span> is. At most 4
        are open at a time; closing the page closes the shell.
      </p>
      <Terminal path="/v1/terminal" autoFocus className="min-h-[20rem] flex-1" />
    </div>
  );
}
