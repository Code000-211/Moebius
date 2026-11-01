/** One short line of what happened: an outcome (`ok`) or a refusal of the server (`error`, with its own sentence). */
import { cn } from "../lib/cn";

export function Notice({ tone, children }: { tone: "ok" | "error"; children: string }) {
  return (
    <p
      role={tone === "error" ? "alert" : "status"}
      className={cn("m-0 font-mono text-[12px]", tone === "error" ? "text-error" : "text-success")}
    >
      {children}
    </p>
  );
}
