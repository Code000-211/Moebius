/** The mark each kind of step carries, in the palette and on a card, so a kind is known by its shape before it is read. */
import {
  ChatCircleIcon,
  CodeIcon,
  GitBranchIcon,
  GitMergeIcon,
  GlobeIcon,
  HandPalmIcon,
  LightningIcon,
  PaperPlaneTiltIcon,
  RepeatIcon,
  RobotIcon,
  SquaresFourIcon,
} from "@phosphor-icons/react";
import type { ComponentType } from "react";

type Mark = ComponentType<{ className?: string; "aria-hidden"?: boolean | "true" | "false" }>;

const marks: Record<string, Mark> = {
  agent: RobotIcon,
  code: CodeIcon,
  condition: GitBranchIcon,
  for_each: RepeatIcon,
  http: GlobeIcon,
  approval: HandPalmIcon,
  merge: GitMergeIcon,
  chat: ChatCircleIcon,
  publish: PaperPlaneTiltIcon,
  trigger: LightningIcon,
};

/** The icon of a kind; a kind the app has no mark for gets a plain square. */
export function KindIcon({ kind, className }: { kind: string; className?: string }) {
  const Icon = marks[kind] ?? SquaresFourIcon;
  return <Icon className={className} aria-hidden="true" />;
}
