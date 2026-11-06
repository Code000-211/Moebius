/**
 * The Ports page: the TCP ports that runs, chats and terminals of the server listen on (`GET /v1/ports`, kept current
 * by `port_opened` and `port_closed`), with whom each belongs to. A client that listens on its own machine can carry every
 * connection over `GET /v1/ports/{port}/tunnel`, a WebSocket of raw bytes; a browser tab
 * cannot open that, so a port comes with the command that forwards it over SSH, and the page says so.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { ports as readPorts } from "../../api/calls/runs";
import { keys } from "../../api/keys";
import { CopyButton } from "../../ui/chat/CopyButton";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { CELL, Table } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { forwardCommand, owner, ownerPath } from "./ports";

export function PortsPage() {
  const ports = useQuery({ queryKey: keys.ports, queryFn: readPorts });
  const host = window.location.hostname;
  return (
    <div data-testid="ports-page">
      <PageHeader
        title="Ports"
        context={
          <>
            The ports runs, chats and terminals listen on. A browser tab cannot reach them, so
            forward one over SSH with the command beside it and open{" "}
            <span className="font-mono text-sm">http://127.0.0.1:&lt;port&gt;</span>.
          </>
        }
      />
      {ports.isPending && <SkeletonLines lines={4} message="Reading the ports" />}
      {ports.isError && <Notice tone="error">Cannot read the ports. Try again.</Notice>}
      {ports.data && ports.data.length === 0 && (
        <div data-testid="no-ports">
          <Empty
            title="No port is open"
            body="A port appears here while a run, chat or terminal listens on one. Nothing listens right now."
          />
        </div>
      )}
      {ports.data && ports.data.length > 0 && (
        <Table head={["Port", "Program", "Belongs to", "Forward over SSH"]}>
          {ports.data.map((port) => {
            const path = ownerPath(port);
            const command = forwardCommand(port.port, host);
            return (
              <tr key={port.port} data-testid="port-row">
                <td className={cn(CELL, "tabular-nums")}>
                  <span className="font-semibold text-text">{port.port}</span>
                </td>
                <td className={CELL}>
                  <span className="break-words text-text">{port.process}</span>{" "}
                  <span className="text-sm text-secondary">
                    pid <span className="tabular-nums">{port.pid}</span>
                  </span>
                </td>
                <td className={CELL}>
                  {path ? (
                    <Link
                      to={path}
                      className="text-text underline underline-offset-4 hover:text-secondary"
                    >
                      {owner(port)}
                    </Link>
                  ) : (
                    <span className="text-text">{owner(port)}</span>
                  )}
                </td>
                <td className={CELL}>
                  <span className="flex flex-wrap items-center gap-2">
                    <code className="break-all font-mono text-sm text-secondary">{command}</code>
                    <CopyButton
                      text={command}
                      label={`Copy the command for port ${port.port}`}
                      className="min-h-10 rounded-control px-3"
                    />
                  </span>
                </td>
              </tr>
            );
          })}
        </Table>
      )}
    </div>
  );
}
