/** Types of the run view, the diffs and the ports; they follow docs/reference/desktop-api.md. */
import type { Run } from "./cards";

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

/** One row of `GET /v1/runs/finished`: a run of a card that has ended, the card it belongs to and the tokens it used. */
export interface FinishedRun {
  run: Run;
  card: {
    id: number;
    repository_id: number;
    number: number;
    project_key: string;
    project_number: number;
    title: string;
  };
  tokens: number;
}
