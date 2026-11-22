/**
 * The layout route of Settings: the page header "Settings" across the top, under it the sub-navigation on the left and
 * the chosen area on the right. It cancels the padding the panel gives a page, so the header and the navigation reach
 * the panel's edges, and gives the area the padding of the mockup back (40px at the sides, 20px above its title), which keeps the sticky header of every page where it was. The
 * layout is as tall as the panel and the area scrolls on its own, so a page that fits the window (the Retro) still does.
 */
import { Suspense } from "react";
import { Outlet } from "react-router";
import { InSettingsArea, PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { settingsEntries } from "../nav";
import { useRole } from "../shell/session";
import { SettingsNav } from "./SettingsNav";

export function SettingsLayout() {
  const role = useRole();
  return (
    <div className="-m-4 flex h-[calc(100%+2rem)] flex-col">
      <div className="shrink-0 px-4 pt-4 [&>header]:mb-0 [&>header]:static">
        <PageHeader title="Settings" />
      </div>
      <div className="flex min-h-0 flex-1 items-stretch">
        <SettingsNav entries={settingsEntries(role)} />
        <div className="min-w-0 flex-1 overflow-y-auto px-10 pb-6 pt-5">
          <InSettingsArea.Provider value>
            <Suspense fallback={<SkeletonCard lines={4} />}>
              <Outlet />
            </Suspense>
          </InSettingsArea.Provider>
        </div>
      </div>
    </div>
  );
}
