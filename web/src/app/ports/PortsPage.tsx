/**
 * The Ports page: the TCP ports that runs, chats and terminals of the server listen on (`GET /v1/ports`, kept current
 * by `port_opened` and `port_closed`), with whom each belongs to. The desktop app opens one by listening on its own
 * machine and carrying every connection over `GET /v1/ports/{port}/tunnel`, a WebSocket of raw bytes; a browser tab
 * cannot open that, so here a port comes with the command that forwards it over SSH, and the page says so.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { ports as readPorts } from "../../api/calls/runs";
import { keys } from "../../api/keys";
import { CopyButton } from "../../ui/chat/CopyButton";
import { Loader } from "../../ui/components/Loader";
import { Header } from "../shell/Page";
import { forwardCommand, owner, ownerPath } from "./ports";

export function PortsPage() {
  const ports = useQuery({ queryKey: keys.ports, queryFn: readPorts });
  const host = window.location.hostname;
  return (
    <div data-testid="ports-page">
      <Header breadcrumb="Setup" title="Ports" />
      <p className="mb-base mt-0 max-w-[60ch] text-xs text-normal">
        The ports that runs, chats and terminals listen on, on the server. The server carries a port
        to the desktop app through a tunnel that a browser tab cannot use, so to open one here,
        forward it over SSH with the command beside it and then open{" "}
        <span className="font-mono">http://127.0.0.1:&lt;port&gt;</span>.
      </p>
      {ports.isPending && <Loader message="Reading the ports…" className="py-double" />}
      {ports.isError && (
        <p role="alert" className="text-sm text-error">
          Cannot read the ports.
        </p>
      )}
      {ports.data && ports.data.length === 0 && (
        <p className="text-sm text-low" data-testid="no-ports">
          No port is open. A port shows here while a run, chat or terminal listens on it.
        </p>
      )}
      {ports.data && ports.data.length > 0 && (
        <table
          className="w-full max-w-[860px] border-collapse text-left text-sm"
          aria-label="Ports"
        >
          <thead>
            <tr className="font-mono text-[11px] text-low">
              <th className="py-half pr-base font-normal">Port</th>
              <th className="py-half pr-base font-normal">Program</th>
              <th className="py-half pr-base font-normal">Belongs to</th>
              <th className="py-half font-normal">Forward over SSH</th>
            </tr>
          </thead>
          <tbody>
            {ports.data.map((port) => {
              const path = ownerPath(port);
              const command = forwardCommand(port.port, host);
              return (
                <tr key={port.port} data-testid="port-row" className="border-t border-line/50">
                  <td className="py-base pr-base font-mono text-high">{port.port}</td>
                  <td className="py-base pr-base text-normal">
                    {port.process}{" "}
                    <span className="font-mono text-xs text-low">pid {port.pid}</span>
                  </td>
                  <td className="py-base pr-base">
                    {path ? (
                      <Link to={path} className="text-accent hover:underline">
                        {owner(port)}
                      </Link>
                    ) : (
                      <span className="text-normal">{owner(port)}</span>
                    )}
                  </td>
                  <td className="py-base">
                    <span className="inline-flex items-center gap-half">
                      <code className="font-mono text-xs text-normal">{command}</code>
                      <CopyButton text={command} label={`Copy the command for port ${port.port}`} />
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}
