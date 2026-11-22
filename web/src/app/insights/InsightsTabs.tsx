/** The tabs of the Insights page, shared by its two pages: Analytics and Usage, each at its own address. */
import { type SectionTab, SectionTabs } from "../shell/SectionTabs";

export const INSIGHT_TABS: readonly SectionTab[] = [
  { path: "insights", label: "Analytics" },
  { path: "insights/usage", label: "Usage" },
];

export function InsightsTabs({ value }: { value: string }) {
  return <SectionTabs tabs={INSIGHT_TABS} value={value} label="Insights" />;
}
