/** The body of a page that is not built in the web app yet. */
import { HammerIcon } from "@phosphor-icons/react";

export function Placeholder({ title }: { title: string }) {
  return (
    <div className="flex flex-col items-start gap-base rounded-xl border border-line bg-surface p-double">
      <HammerIcon className="size-icon-sm text-low" weight="bold" />
      <p className="text-sm text-normal">
        <span className="text-high">{title}</span> is coming in the web app soon. The desktop app
        has it today.
      </p>
    </div>
  );
}
