/**
 * The page the end-to-end test of the list opens: 2000 rows (and a group row to every hundred) in a `VirtualList`
 * inside a panel of a fixed height. It is built by the test itself (`../list.e2e.ts`) and is not part of the app: no
 * route leads to it and `pnpm build` does not include it.
 */

import { useState } from "react";
import { createRoot } from "react-dom/client";
import "../../src/styles/tailwind.css";
import { ListRow } from "../../src/ui/page/ListRow";
import { type ListItem, VirtualList } from "../../src/ui/page/VirtualList";

const ROWS = 2000;

const items: ListItem<number>[] = [];
for (let n = 0; n < ROWS; n++) {
  if (n % 100 === 0) items.push({ type: "group", key: `group-${n}`, name: `Group ${n / 100 + 1}` });
  items.push({ type: "row", key: `row-${n}`, data: n });
}

function Fixture() {
  const [selected, setSelected] = useState<string | null>("row-0");
  return (
    <main className="flex h-screen flex-col bg-panel text-text">
      <VirtualList
        label="Rows"
        items={items}
        selectedKey={selected}
        onSelect={setSelected}
        renderRow={(n, { at, selected: on, onClick }) => (
          <ListRow
            at={at}
            selected={on}
            onClick={onClick}
            status={n % 3 === 0 ? "needsYou" : n % 3 === 1 ? "running" : "done"}
            id={`MOE-${n}`}
            title={`Row number ${n} with a title that goes on`}
            reason={n === 0 ? "Review found a data loss" : "editing engine.rs"}
            owner={n % 2 === 0 ? "leon" : undefined}
            age="2m"
            action={
              n === 0 ? (
                <button
                  type="button"
                  className="h-6 rounded-control px-2.5 text-base text-accent-text outline outline-1 -outline-offset-1 outline-hairline-strong"
                >
                  Approve plan
                </button>
              ) : undefined
            }
          />
        )}
      />
    </main>
  );
}

const root = document.getElementById("root");
if (root) createRoot(root).render(<Fixture />);
