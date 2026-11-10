/**
 * The package sheet: a floating panel like the floating chat, in which a person searches a catalog, reads a package and
 * installs, updates or removes it. The shell opens it on the Moebius registries (from the Plugins page) or on the
 * pi.dev catalog (from an agent's Extensions); one sheet at a time, so opening it from elsewhere switches its catalog.
 * It takes the slot at the right of the window that the floating chat shares (opening one minimises the other), and keeps
 * its state when minimised. What a request told, or why
 * it was refused, is shown in the sheet, never as a broken page.
 */
import { PackageIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { snapshot as readSnapshot } from "../api/calls/board";
import { installedPi } from "../api/calls/packages";
import { keys } from "../api/keys";
import type { PiHit } from "../api/types/packages";
import { ScrollFades } from "../ui/page/ScrollFades";
import { SheetFrame, SheetOpener, useSheetBehaviour } from "../ui/page/Sheet";
import { TabPanel, Tabs } from "../ui/page/Tabs";
import { Browse, type Browsing } from "./Browse";
import { Installed } from "./Installed";
import { MoebiusDetail } from "./MoebiusDetail";
import { type Catalog, piSource, waitingPiUpdates } from "./model";
import { usePackageSheet } from "./PackageSheetContext";
import { PiDetail } from "./PiDetail";
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
  hit?: PiHit;
  /** Where an installed Pi package came from (npm, git); a package found by search is an npm package. */
  source?: string;
  from: Tab;
}

function Body({
  catalog,
  agent,
  shown,
  hide,
}: {
  catalog: Catalog;
  /** Whether the sheet is showing: it keeps its state while minimised. */
  shown: boolean;
  agent: string | null;
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
  const pi = useQuery({
    queryKey: keys.piInstalled,
    queryFn: installedPi,
    enabled: catalog === "pi",
    retry: false,
  });
  const waiting = useMemo(() => waitingPiUpdates(board.data?.inbox ?? []), [board.data]);
  const installed = useMemo(
    () => new Map((pi.data ?? []).map((each) => [each.name, each.version])),
    [pi.data],
  );
  const repositories = (board.data?.boards ?? []).map((each) => each.repository.name);

  const open = (name: string, from: Tab, extra: { hit?: PiHit; source?: string } = {}) => {
    setReport(null);
    setReading({ name, from, ...extra });
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
      <ScrollFades className="min-h-0 flex-1">
        <TabPanel idBase={ids} value={tab} className="flex flex-col gap-3 pb-6">
          {report && <ReportView report={report} onLeave={hide} />}
          {reading !== null ? (
            catalog === "moebius" ? (
              <MoebiusDetail
                name={reading.name}
                actions={actions}
                repositories={repositories}
                back={backLabel}
                onBack={back}
              />
            ) : (
              <PiDetail
                name={reading.name}
                hit={reading.hit}
                source={reading.source ?? piSource(reading.name)}
                agent={agent}
                actions={actions}
                back={backLabel}
                onBack={back}
                waiting={waiting.get(reading.name)?.to ?? null}
              />
            )
          ) : tab === "browse" ? (
            <Browse
              catalog={catalog}
              browsing={browsing}
              onBrowsing={setBrowsing}
              onOpen={(name, hit) => open(name, "browse", { hit })}
              installed={installed}
              waiting={waiting}
              searchRef={searchRef}
            />
          ) : (
            <Installed
              catalog={catalog}
              actions={actions}
              waiting={waiting}
              onOpen={(name, source) => open(name, "installed", { source })}
            />
          )}
        </TabPanel>
      </ScrollFades>
    </div>
  );
}

export function PackageSheet() {
  const sheet = usePackageSheet();
  const opener = useSheetBehaviour(sheet.shown, sheet.hide);
  if (!sheet.opened) return null;
  const line =
    sheet.catalog === "moebius"
      ? "Plugins, workflows, rules and skills from the registries"
      : sheet.agent === null
        ? "Pi packages from pi.dev"
        : `Pi packages from pi.dev, for ${sheet.agent}`;
  return (
    <>
      {!sheet.shown && (
        <SheetOpener
          openerRef={opener}
          label="Open the package sheet"
          title="Browse packages"
          onOpen={() => sheet.open(sheet.catalog, sheet.agent)}
          icon={<PackageIcon aria-hidden="true" className="size-5" weight="fill" />}
          className="right-20"
        />
      )}
      <SheetFrame
        label="Package sheet"
        title="Packages"
        line={line}
        minimiseLabel="Minimise the package sheet"
        onMinimise={sheet.hide}
        hidden={!sheet.shown}
        // The same width as the floating chat, which shares the slot (`Sheet.tsx`): wider from xl, widest from 2xl,
        // with the page's paddings (`Shell.tsx`, `ChatPage.tsx`) clear of its edge.
        className="top-6 right-6 bottom-24 w-96 xl:w-[26rem] 2xl:w-[30rem]"
      >
        {/* A catalog of its own is a sheet of its own: switching it starts with a clean search and its own tabs. */}
        <Body
          key={sheet.catalog}
          catalog={sheet.catalog}
          agent={sheet.agent}
          shown={sheet.shown}
          hide={sheet.hide}
        />
      </SheetFrame>
    </>
  );
}
