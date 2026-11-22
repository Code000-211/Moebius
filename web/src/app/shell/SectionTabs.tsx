/**
 * The tabs of a page that is one place with several views, each at an address of its own (Agents | Skills | MCP
 * servers). The tab is the address: choosing one navigates, so a link and a reload keep it, and Back returns to the
 * tab before. A page puts the row in its `PageHeader` as `tabs`, with the tabs it shares and the one it is.
 */
import { useId } from "react";
import { useNavigate } from "react-router";
import { Tabs } from "../../ui/page/Tabs";

export interface SectionTab {
  /** The path, without the leading slash. */
  path: string;
  label: string;
}

export function SectionTabs({
  tabs,
  value,
  label,
}: {
  tabs: readonly SectionTab[];
  /** The path of the tab the page is. */
  value: string;
  /** What the row of tabs is for, for the person hearing the page read aloud. */
  label: string;
}) {
  const base = useId();
  const navigate = useNavigate();
  return (
    <Tabs
      tabs={tabs.map((tab) => ({ value: tab.path, label: tab.label }))}
      value={value}
      onChange={(path) => navigate(`/${path}`)}
      label={label}
      idBase={base}
    />
  );
}
