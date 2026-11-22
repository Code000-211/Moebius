/** The tabs of the Improve page, shared by its four pages: Retro, Lessons, Experiments and Decisions, each at its own address. */
import { type SectionTab, SectionTabs } from "../shell/SectionTabs";

export const IMPROVE_TABS: readonly SectionTab[] = [
  { path: "improve", label: "Retro" },
  { path: "improve/lessons", label: "Lessons" },
  { path: "improve/experiments", label: "Experiments" },
  { path: "improve/decisions", label: "Decisions" },
];

export function ImproveTabs({ value }: { value: string }) {
  return <SectionTabs tabs={IMPROVE_TABS} value={value} label="Improve" />;
}
