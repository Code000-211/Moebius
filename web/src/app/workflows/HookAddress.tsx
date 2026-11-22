/**
 * The address a workflow on a `webhook` trigger answers on, with the project in it (ADR 0025), and a button that
 * copies it. It is shown on the workflow's row in the project's Workflows tab and on its Overview: the Webhooks panel of Settings holds
 * only the forges' hooks, and a workflow's hook belongs to its project, so a hook of a private project is only ever
 * shown to someone who can open that project.
 */
import { CopyIcon } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../ui/components/Button";

/** Copies `address` to the clipboard and says how it went for a moment: `idle`, `copied` or `failed`. */
function useCopy(address: string) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  async function copy() {
    let next: "copied" | "failed" = "copied";
    try {
      await navigator.clipboard.writeText(address);
    } catch {
      next = "failed";
    }
    setState(next);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setState("idle"), 1500);
  }
  return { state, copy };
}

/** The "Copy hook URL" of a webhook workflow's page header: the same address the Workflows tab shows, copied. */
export function CopyHookUrl({ address }: { address: string }) {
  const { state, copy } = useCopy(address);
  return (
    <Button variant="ghost" size="sm" disabled={state !== "idle"} onClick={() => void copy()}>
      <CopyIcon className="size-4" aria-hidden="true" />
      <span aria-live="polite">
        {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy hook URL"}
      </span>
    </Button>
  );
}

export function HookAddress({ workflow, address }: { workflow: string; address: string }) {
  const { state, copy } = useCopy(address);

  return (
    <span className="flex min-w-0 shrink items-center gap-1.5">
      <code
        data-testid="hook-address"
        className="min-w-0 truncate rounded-round bg-hover px-1.5 py-0.5 font-mono text-xs text-text-secondary"
      >
        {address}
      </code>
      <Button
        variant="ghost"
        size="sm"
        aria-label={`Copy the webhook address of ${workflow}`}
        disabled={state !== "idle"}
        onClick={() => void copy()}
      >
        <CopyIcon className="size-4" aria-hidden="true" />
        <span aria-live="polite">
          {state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : "Copy"}
        </span>
      </Button>
    </span>
  );
}
