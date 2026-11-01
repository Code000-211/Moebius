/** Types of the run view, the diffs and the ports; they follow docs/reference/desktop-api.md. */

/** `GET /v1/runs/{id}/diff` and `GET /v1/changes/{id}/diff`: a unified diff with secrets removed. */
export interface Diff {
  /** Empty when nothing changed or the branch is gone. */
  diff: string;
  /** Whether the server cut the diff at 2 000 000 bytes. */
  truncated: boolean;
}

/** One entry of `GET /v1/ports`: a TCP port that a run, a chat or a terminal listens on. */
export interface Port {
  port: number;
  pid: number;
  /** The program's name. */
  process: string;
  kind: "run" | "chat" | "terminal";
  /** The run or chat; for a terminal, the one it was opened on, null for the server's own terminal. */
  run: number | null;
}
