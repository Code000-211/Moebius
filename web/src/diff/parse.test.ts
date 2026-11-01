import { describe, expect, it } from "vitest";
import { parseUnifiedDiff, sideBySide, totals } from "./parse";

const DIFF = `diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@ fn main
 fn main() {
-    println!("old");
+    println!("new");
+    println!("more");
 }
@@ -20,2 +21,2 @@
 last
-tail
\\ No newline at end of file
+tail again
diff --git a/feature.txt b/feature.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/feature.txt
@@ -0,0 +1 @@
+done
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 4444444..0000000
--- a/gone.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-a
-b
diff --git a/old name.txt b/new name.txt
similarity index 90%
rename from old name.txt
rename to new name.txt
diff --git a/logo.png b/logo.png
new file mode 100644
Binary files /dev/null and b/logo.png differ
`;

describe("parseUnifiedDiff", () => {
  const files = parseUnifiedDiff(DIFF);

  it("finds the files in order with their status", () => {
    expect(files.map((file) => [file.path, file.status])).toEqual([
      ["src/main.rs", "modified"],
      ["feature.txt", "added"],
      ["gone.txt", "deleted"],
      ["new name.txt", "renamed"],
      ["logo.png", "added"],
    ]);
    expect(files[3]?.oldPath).toBe("old name.txt");
    expect(files[4]?.binary).toBe(true);
  });

  it("counts the added and removed lines", () => {
    expect(files[0]).toMatchObject({ additions: 3, deletions: 2 });
    expect(files[1]).toMatchObject({ additions: 1, deletions: 0 });
    expect(files[2]).toMatchObject({ additions: 0, deletions: 2 });
    expect(totals(files)).toEqual({ additions: 4, deletions: 4 });
  });

  it("numbers the lines of each hunk in the old and the new file", () => {
    const [first, second] = files[0]?.hunks ?? [];
    expect(first?.header).toBe("@@ -1,4 +1,5 @@ fn main");
    expect(first?.lines.map((line) => [line.kind, line.oldNumber, line.newNumber])).toEqual([
      ["context", 1, 1],
      ["removed", 2, null],
      ["added", null, 2],
      ["added", null, 3],
      ["context", 3, 4],
    ]);
    expect(second?.lines.map((line) => line.kind)).toEqual(["context", "removed", "note", "added"]);
    expect(second?.lines[1]).toMatchObject({ text: "tail", oldNumber: 21 });
    expect(second?.lines[3]).toMatchObject({ text: "tail again", newNumber: 22 });
  });

  it("keeps the text of a line whole, without its sign", () => {
    expect(files[0]?.hunks[0]?.lines[1]?.text).toBe('    println!("old");');
  });

  it("reads nothing from text that is no diff", () => {
    expect(parseUnifiedDiff("")).toEqual([]);
    expect(parseUnifiedDiff("hello\n+not a diff\n")).toEqual([]);
  });
});

describe("sideBySide", () => {
  const lines = parseUnifiedDiff(DIFF)[0]?.hunks[0]?.lines ?? [];

  it("sets a removed run against the added run that follows it", () => {
    const rows = sideBySide(lines);
    expect(rows.map((row) => [row.left?.text ?? null, row.right?.text ?? null])).toEqual([
      ["fn main() {", "fn main() {"],
      ['    println!("old");', '    println!("new");'],
      [null, '    println!("more");'],
      ["}", "}"],
    ]);
  });

  it("leaves the rest of a longer removed run against nothing", () => {
    const rows = sideBySide(parseUnifiedDiff(DIFF)[2]?.hunks[0]?.lines ?? []);
    expect(rows.map((row) => [row.left?.text, row.right])).toEqual([
      ["a", null],
      ["b", null],
    ]);
  });
});
