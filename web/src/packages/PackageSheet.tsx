/**
 * The package sheet: a sheet in which a person searches the registries of Moebius, reads a package and installs,
 * updates or removes it. A page opens it (the Plugins page). It lies over the page's right column, below the page
 * header, and keeps its state while hidden, until the page opens it again. The Pi packages of pi.dev are the
 * Extensions tab of an agent's page, not this sheet. What a request told, or why it was refused, is shown in the sheet, never as a
 * broken page.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { snapshot as readSnapshot } from "../api/calls/board";
import { keys } from "../api/keys";
import { SheetFrame, useSheetBehaviour } from "../ui/page/Sheet";
import { TabPanel, Tabs } from "../ui/page/Tabs";
import { Browse, type Browsing } from "./Browse";
import { Installed } from "./Installed";
import { MoebiusDetail } from "./MoebiusDetail";
import { usePackageSheet } from "./PackageSheetContext";
import { type Report, ReportView } from "./Report";
import { useActions } from "./useActions";

type Tab = "browse" | "installed";

const TABS: Array<{ value: Tab; label: string }> = [
  { value: "browse", label: "Browse" },
  { value: "installed", label: "Installed" },
];

/** A package the person is reading: the tab it was opened from, so Back goes there. */
interface Reading {
  name: string;
  from: Tab;
}

function Body({
  shown,
  hide,
}: {
  /** Whether the sheet is showing: it keeps its state while hidden. */
  shown: boolean;
  hide: () => void;
}) {
  const ids = useId();
  const [tab, setTab] = useState<Tab>("browse");
  const [browsing, setBrowsing] = useState<Browsing>({ query: "", kind: null });
  const [reading, setReading] = useState<Reading | null>(null);
  const [report, setReport] = useState<Report | null>(null);
  const actions = useActions(setReport);
  const searchRef = useRef<HTMLInputElement>(null);
  // The search is what a person opened the sheet for: the focus goes to it when the sheet appears, not when a package
  // is being read or the Installed tab is chosen, which have no field.
  useEffect(() => {
    if (shown) searchRef.current?.focus();
  }, [shown]);
  const board = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const repositories = (board.data?.boards ?? []).map((each) => each.repository.name);

  const open = (name: string, from: Tab) => {
    setReport(null);
    setReading({ name, from });
  };
  const choose = (next: Tab) => {
    setReport(null);
    setReading(null);
    setTab(next);
  };
  const back = () => setReading(null);
  const backLabel = reading?.from === "installed" ? "Installed" : "Results";

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <Tabs tabs={TABS} value={tab} onChange={choose} label="Packages" idBase={ids} />
      <div className="min-h-0 min-w-0 flex-1 overflow-auto">
        <TabPanel idBase={ids} value={tab} className="flex flex-col gap-3 pb-6">
          {report && <ReportView report={report} onLeave={hide} />}
          {reading !== null ? (
            <MoebiusDetail
              name={reading.name}
              actions={actions}
              repositories={repositories}
              back={backLabel}
              onBack={back}
            />
          ) : tab === "browse" ? (
            <Browse
              browsing={browsing}
              onBrowsing={setBrowsing}
              onOpen={(name) => open(name, "browse")}
              searchRef={searchRef}
            />
          ) : (
            <Installed actions={actions} onOpen={(name) => open(name, "installed")} />
          )}
        </TabPanel>
      </div>
    </div>
  );
}

export function PackageSheet() {
  const sheet = usePackageSheet();
  useSheetBehaviour(sheet.shown, sheet.hide);
  if (!sheet.opened) return null;
  return (
    <SheetFrame
      label="Package sheet"
      title="Packages"
      line="Plugins, workflows, rules and skills from the registries"
      closeLabel="Close the package sheet"
      onClose={sheet.hide}
      hidden={!sheet.shown}
    >
      <Body shown={sheet.shown} hide={sheet.hide} />
    </SheetFrame>
  );
}
