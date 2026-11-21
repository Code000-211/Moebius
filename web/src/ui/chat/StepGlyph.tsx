/**
 * The small icon at the head of a step of the thread, picked by what the step did (`stepWords` in
 * `chat/conversation/tools.ts`). It sits on the thin line the steps hang on, so it carries the `step-mark` class that
 * lets the line break around it.
 */
import {
  FileTextIcon,
  GlobeIcon,
  ListMagnifyingGlassIcon,
  NotePencilIcon,
  ShieldCheckIcon,
  TerminalWindowIcon,
  TestTubeIcon,
  WrenchIcon,
} from "@phosphor-icons/react";
import type { StepIcon } from "../../chat/conversation";
import { cn } from "../lib/cn";

const ICONS = {
  read: FileTextIcon,
  edit: NotePencilIcon,
  terminal: TerminalWindowIcon,
  search: ListMagnifyingGlassIcon,
  test: TestTubeIcon,
  check: ShieldCheckIcon,
  web: GlobeIcon,
  tool: WrenchIcon,
} as const;

export function StepGlyph({ icon, className }: { icon: StepIcon; className?: string }) {
  const Glyph = ICONS[icon];
  return <Glyph aria-hidden="true" className={cn("step-mark size-4 shrink-0", className)} />;
}

/** The words of a step: the plain sentence, and the file it is about in mono. */
export function StepText({ text, code }: { text: string; code?: string | undefined }) {
  return (
    <span className="min-w-0 truncate text-sm">
      {text}
      {code && <span className="ml-1 font-mono text-[12px]">{code}</span>}
    </span>
  );
}
