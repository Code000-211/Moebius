/**
 * The sub-navigation of Settings: the entries in their groups, each with its icon (`design/navigation/ia4.png`). There
 * is no search field: nine entries are found by reading them. Entries come from the table of `nav.ts`.
 */
import { NavLink } from "react-router";
import { groupEntries, type SettingsEntry } from "../nav";
import { SUB_NAV_GROUP, SubNavLabel, subNavRow } from "./SubNav";

export function SettingsNav({ entries }: { entries: readonly SettingsEntry[] }) {
  return (
    <nav
      aria-label="Settings"
      className="flex w-50 shrink-0 flex-col gap-3 overflow-y-auto border-r border-hairline px-2 py-2"
    >
      {groupEntries(entries).map(({ group, entries: inGroup }) => (
        <div key={group} className="flex flex-col gap-0.5">
          <span className={SUB_NAV_GROUP}>{group}</span>
          {inGroup.map((entry) => (
            <NavLink
              key={entry.path}
              to={`/${entry.path}`}
              className={({ isActive }) => subNavRow(isActive)}
            >
              <SubNavLabel icon={entry.icon} label={entry.label} />
            </NavLink>
          ))}
        </div>
      ))}
    </nav>
  );
}
