/**
 * The layout route of Settings: the sub-navigation on the left and the chosen page on the right. It cancels the
 * padding the panel gives a page, so the navigation reaches the panel's edges, and gives the page the padding back,
 * which keeps the sticky header of every page where it was. The layout is as tall as the panel and its two columns scroll on their own, so a page that fits the window (the Retro) still does.
 */
import { Suspense } from "react";
import { Outlet } from "react-router";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { useRole } from "../shell/session";
import { SettingsNav } from "./SettingsNav";
import { shownEntries } from "./settingsEntries";

export function SettingsLayout() {
  const role = useRole();
  return (
    <div className="-m-6 flex h-[calc(100%+3rem)] items-stretch">
      <SettingsNav entries={shownEntries(role)} />
      <div className="min-w-0 flex-1 overflow-y-auto p-6">
        <Suspense fallback={<SkeletonCard lines={4} />}>
          <Outlet />
        </Suspense>
      </div>
    </div>
  );
}
