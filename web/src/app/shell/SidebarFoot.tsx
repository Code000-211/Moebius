/**
 * The foot of the sidebar: Settings, alone, with its word. It is the one row the sidebar never exchanges, so a hand
 * that learned its place keeps it on every page, Settings pages included.
 */

import { Link } from "react-router";
import { SettingsIcon } from "../../ui/icons";
import { cn } from "../../ui/lib/cn";
import { ROW, rowTone } from "./SidebarEntry";

export function SettingsFoot({ current }: { current: boolean }) {
  return (
    <Link
      to="/settings"
      aria-current={current ? "page" : undefined}
      className={cn(ROW, rowTone(current))}
    >
      <SettingsIcon className="size-[18px] shrink-0" weight="regular" />
      <span className="min-w-0 flex-1 truncate">Settings</span>
    </Link>
  );
}
