// The Moebius lens for Pi-Bolt (issue 114): the agent sees its own mistakes at once. Moebius loads this file into each run
// with `--extension`, before the bridge, and sets MOEBIUS_LENS (the plan: what to run, from the repository's
// `.moebius/lint.toml` or detected), MOEBIUS_CHECK_URL and MOEBIUS_RUN_TOKEN for the Pi process. Without a plan it
// does nothing.
// After an edit of a file (the `edit` and `write` tools), within a time budget, it runs a syntax check and the
// repository's fast linter on that file alone, and appends to the tool's result only the problems that are new: the
// file's diagnostics before the edit are the baseline. Before the run settles it runs the slow type check once, for
// the files the run changed, and tells the agent in a message. It never starts a language server.
// Which findings are shown and whether the slow check runs are Moebius's decisions (`decide`); when the server cannot
// be asked, the lens takes the safe side: it shows the finding and runs the check. A failure of the lens itself never
// stops a call or the run.

import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

type Severity = "error" | "warning";

interface Check {
	name: string;
	files: string;
	edit: string | null;
	deep: string | null;
	syntax: string | null;
	format: "lines" | "files" | "diff-in" | "github";
	pattern: string | null;
	severity: Severity;
}

interface Plan {
	enabled: boolean;
	budget_ms: number;
	max_lines: number;
	deep_timeout_ms: number;
	syntax: boolean;
	checks: Check[];
	error: string | null;
}

interface Finding {
	tool: string;
	severity: Severity;
	file: string;
	line: number;
	column?: number;
	rule?: string;
	message: string;
	touched: boolean;
	key: string;
}

interface Timing {
	phase: "syntax" | "lint" | "deep";
	tool: string;
	ms: number;
	skipped?: string;
	findings: number;
	shown?: number;
}

// The most rounds of the end-of-turn message in one run, so that a check that cannot be satisfied does not loop.
const DEEP_ROUNDS = 3;

function loadPlan(): Plan | undefined {
	try {
		const plan = JSON.parse(process.env.MOEBIUS_LENS ?? "") as Plan;
		return plan && typeof plan === "object" ? plan : undefined;
	} catch {
		return undefined;
	}
}

// POSTs `body` to a Moebius lens endpoint and returns the parsed answer; throws when there is none.
async function ask(endpoint: string, body: unknown): Promise<any> {
	const url = process.env.MOEBIUS_CHECK_URL;
	const token = process.env.MOEBIUS_RUN_TOKEN;
	if (!url || !token) throw new Error("MOEBIUS_CHECK_URL or MOEBIUS_RUN_TOKEN is not set");
	const response = await fetch(`${url}/moebius/lens/${endpoint}`, {
		method: "POST",
		headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
		body: JSON.stringify(body),
	});
	if (!response.ok) throw new Error(`HTTP ${response.status}`);
	return await response.json();
}

// The words of a command line, with 'single' and "double" quotes and backslashes inside double quotes.
function words(text: string): string[] {
	const out: string[] = [];
	let current = "";
	let quote: string | null = null;
	let started = false;
	for (let i = 0; i < text.length; i++) {
		const c = text[i];
		if (quote === "'") {
			if (c === "'") quote = null;
			else current += c;
		} else if (quote === '"') {
			if (c === '"') quote = null;
			else if (c === "\\" && i + 1 < text.length) current += text[++i];
			else current += c;
		} else if (c === "'" || c === '"') {
			quote = c;
			started = true;
		} else if (/\s/.test(c)) {
			if (started || current !== "") out.push(current);
			current = "";
			started = false;
		} else {
			current += c;
			started = true;
		}
	}
	if (started || current !== "") out.push(current);
	return out;
}

// The command's arguments with `{file}` replaced by the file, and an argument that is exactly `{files}` or `{dirs}`
// by one argument for each file or directory.
function expand(command: string, vars: { file?: string; files?: string[]; dirs?: string[] }): string[] {
	const out: string[] = [];
	for (const word of words(command)) {
		if (word === "{files}") out.push(...(vars.files ?? []));
		else if (word === "{dirs}") out.push(...(vars.dirs ?? []));
		else out.push(word.split("{file}").join(vars.file ?? ""));
	}
	return out;
}

interface Ran {
	output: string;
	missing: boolean;
	timedOut: boolean;
}

// Runs `argv` in `cwd`, without a shell, for at most `timeoutMs`. Its standard output and error are one text.
function run(argv: string[], cwd: string, timeoutMs: number): Promise<Ran> {
	return new Promise((resolve) => {
		let program = argv[0];
		if (!program.includes("/")) {
			const local = path.join(cwd, "node_modules", ".bin", program);
			if (fs.existsSync(local)) program = local;
		}
		let output = "";
		let timedOut = false;
		let done = false;
		const finish = (missing: boolean) => {
			if (done) return;
			done = true;
			clearTimeout(timer);
			resolve({ output, missing, timedOut });
		};
		let child: ReturnType<typeof spawn>;
		try {
			child = spawn(program, argv.slice(1), { cwd, stdio: ["ignore", "pipe", "pipe"] });
		} catch {
			resolve({ output, missing: true, timedOut });
			return;
		}
		const timer = setTimeout(() => {
			timedOut = true;
			try {
				child.kill("SIGKILL");
			} catch {}
			finish(false);
		}, Math.max(1, timeoutMs));
		const take = (chunk: Buffer) => {
			if (output.length < 1_000_000) output += chunk.toString("utf8");
		};
		child.stdout?.on("data", take);
		child.stderr?.on("data", take);
		child.on("error", (error: any) => finish(error?.code === "ENOENT"));
		child.on("close", () => finish(false));
	});
}

const LINE = /^(?<file>[^\s:][^:]*?):(?<line>\d+)(?::(?<column>\d+))?:?\s*(?<message>.*)$/;

// The rule a message names at its end or inside, if it does.
function ruleIn(message: string): string | undefined {
	const patterns = [/\[([A-Za-z]{1,6}\d{2,5})\]\s*$/, /\[(?:Error|Warning)\/([\w/-]+)\]\s*$/, /\((report\w+)\)\s*$/];
	for (const pattern of patterns) {
		const match = pattern.exec(message);
		if (match) return match[1];
	}
	return undefined;
}

function severityOf(text: string, fallback: Severity): Severity {
	if (/\b(?:error|fatal)\b|SyntaxError/i.test(text)) return "error";
	if (/\bwarn(?:ing)?\b/i.test(text)) return "warning";
	return fallback;
}

// A path in a linter's output as a path from the worktree.
function relative(cwd: string, file: string): string {
	let name = file.trim().replace(/\\/g, "/");
	if (path.isAbsolute(name)) {
		const rel = path.relative(cwd, name).replace(/\\/g, "/");
		if (!rel.startsWith("..")) name = rel;
	}
	return name.replace(/^\.\//, "");
}

function finding(tool: string, file: string, line: number, column: number | undefined, severity: Severity, message: string, rule?: string): Finding {
	message = message.replace(/\s+/g, " ").trim().slice(0, 300);
	// A leading severity word, and the rule after it, are told apart from the message: `error[E0425]: ...`, `error TS2322: ...`.
	const lead = /^(?:error|warning|note)(?:\[([^\]]+)\])?:?\s*/i.exec(message);
	if (lead) {
		rule = rule ?? lead[1];
		message = message.slice(lead[0].length);
	}
	// A rule the message starts with: `F401 [*] ...`, `L001 ...`, `TS2322: ...`.
	const first = /^([A-Za-z]{1,6}\d{2,5})\b:?\s*(?:\[\*\]\s*)?/.exec(message);
	if (first && rule === undefined) {
		rule = first[1];
		message = message.slice(first[0].length);
	}
	rule = rule ?? ruleIn(message);
	return { tool, severity, file, line, column, rule, message, touched: false, key: `${tool}|${rule ?? ""}|${message}` };
}

// The findings in the output of a check's command.
function parse(output: string, check: Check, tool: string, cwd: string): Finding[] {
	const found: Finding[] = [];
	let custom: RegExp | undefined;
	try {
		if (check.pattern) custom = new RegExp(check.pattern);
	} catch {}
	for (const raw of output.split(/\r?\n/)) {
		if (check.format === "diff-in") {
			const m = /^Diff in (.+?):(\d+)/.exec(raw);
			if (m) found.push(finding(tool, relative(cwd, m[1]), Number(m[2]), undefined, check.severity, "formatting differs from the formatter's", "format"));
		} else if (check.format === "github") {
			const m = /^::(error|warning|notice)\s+([^:]*)::(.*)$/.exec(raw);
			if (!m) continue;
			const props = (name: string) => new RegExp(`(?:^|,)${name}=([^,]*)`).exec(m[2])?.[1];
			const file = props("file");
			if (!file) continue;
			found.push(
				finding(tool, relative(cwd, file), Number(props("line") ?? 1), props("col") ? Number(props("col")) : undefined, m[1] === "error" ? "error" : "warning", m[3], props("title")),
			);
		} else {
			const m = (custom ?? LINE).exec(raw);
			if (m?.groups?.file && m.groups.line) {
				const message = m.groups.message ?? "";
				const severity = m.groups.severity ? severityOf(m.groups.severity, check.severity) : severityOf(message, check.severity);
				found.push(finding(tool, relative(cwd, m.groups.file), Number(m.groups.line), m.groups.column ? Number(m.groups.column) : undefined, severity, message));
			} else if (check.format === "files" && raw.trim() !== "" && !/\s/.test(raw.trim())) {
				found.push(finding(tool, relative(cwd, raw), 1, undefined, check.severity, "is not formatted", "format"));
			}
		}
	}
	return found;
}

// The syntax checks that need no configuration: a parser that is cheaply there.
const PYTHON_SYNTAX = [
	"import ast, sys",
	"name = sys.argv[1]",
	"try:",
	"    ast.parse(open(name, encoding='utf-8').read(), name)",
	"except SyntaxError as e:",
	"    print(f'{name}:{e.lineno or 1}:{e.offset or 1}: SyntaxError: {e.msg}')",
].join("\n");

function builtinSyntax(file: string): { argv: string[]; check: Check } | undefined {
	const base: Check = { name: "syntax", files: "", edit: null, deep: null, syntax: null, format: "lines", pattern: null, severity: "error" };
	const extension = path.extname(file).toLowerCase();
	if (extension === ".py") return { argv: ["python3", "-c", PYTHON_SYNTAX, file], check: base };
	if (extension === ".sh" || extension === ".bash") {
		const pattern = "^(?<file>.+?): line (?<line>\\d+): (?<message>.*)$";
		return { argv: ["bash", "-n", file], check: { ...base, pattern } };
	}
	return undefined;
}

function jsonSyntax(file: string, text: string): Finding[] {
	try {
		JSON.parse(text);
		return [];
	} catch (error: any) {
		const message = String(error?.message ?? error);
		const at = /position (\d+)/.exec(message);
		const line = at ? text.slice(0, Number(at[1])).split("\n").length : 1;
		return [finding("syntax", file, line, undefined, "error", `SyntaxError: ${message}`, "json")];
	}
}

const COMMENTS: Record<string, string[]> = {
	slash: ["//", "/*", "*"],
	hash: ["#"],
	dash: ["--"],
};
const COMMENT_STYLE: Record<string, string> = {};
for (const e of "rs go js jsx ts tsx mjs cjs java c h cc cpp hpp cs kt swift scala php dart".split(" ")) COMMENT_STYLE[`.${e}`] = "slash";
for (const e of "py rb sh bash pl r ex exs".split(" ")) COMMENT_STYLE[`.${e}`] = "hash";
for (const e of "lua sql hs".split(" ")) COMMENT_STYLE[`.${e}`] = "dash";

interface Change {
	// The first and last line of the file after the edit that differ, from 1.
	first: number;
	last: number;
	// Whether the lines that changed are blank or comments in both versions.
	commentOnly: boolean;
	excerpt: string;
}

// What an edit changed: the lines between the text that stayed the same at the start and at the end.
function changeOf(file: string, before: string | null, after: string): Change {
	const lines = (text: string) => {
		const all = text.split("\n");
		if (all[all.length - 1] === "") all.pop();
		return all;
	};
	const a = lines(after);
	if (before === null) return { first: 1, last: a.length, commentOnly: false, excerpt: after };
	const b = lines(before);
	let head = 0;
	while (head < b.length && head < a.length && b[head] === a[head]) head++;
	let tail = 0;
	while (tail < b.length - head && tail < a.length - head && b[b.length - 1 - tail] === a[a.length - 1 - tail]) tail++;
	const now = a.slice(head, a.length - tail);
	const was = b.slice(head, b.length - tail);
	const prefixes = COMMENTS[COMMENT_STYLE[path.extname(file).toLowerCase()] ?? ""];
	const comment = (line: string) => {
		const t = line.trim();
		return t === "" || (prefixes?.some((p) => t.startsWith(p)) ?? false);
	};
	const first = now.length === 0 ? Math.max(1, head) : head + 1;
	const last = now.length === 0 ? Math.min(a.length, head + 1) : head + now.length;
	return { first, last, commentOnly: prefixes !== undefined && now.every(comment) && was.every(comment), excerpt: now.join("\n") };
}

// Each finding that is new in `after` against `before`, by what a finding is without its line; of equal findings, the
// ones off the touched lines are the old ones. Without a `before`, the findings on the touched lines are new.
function newOnes(after: Finding[], before: Finding[] | undefined): Finding[] {
	if (before === undefined) return after.filter((f) => f.touched);
	const old = new Map<string, number>();
	for (const f of before) old.set(f.key, (old.get(f.key) ?? 0) + 1);
	const fresh: Finding[] = [];
	const sorted = [...after].sort((x, y) => Number(x.touched) - Number(y.touched) || x.line - y.line);
	for (const f of sorted) {
		const left = old.get(f.key) ?? 0;
		if (left > 0) old.set(f.key, left - 1);
		else fresh.push(f);
	}
	return fresh;
}

function describe(f: Finding): string {
	const where = `${f.file}:${f.line}${f.column ? `:${f.column}` : ""}`;
	const rule = f.rule ? ` ${f.rule}` : "";
	return `${where} ${f.severity}${rule} ${f.message} (${f.tool})`;
}

export default function (pi: ExtensionAPI) {
	const plan = loadPlan();
	if (!plan || !plan.enabled) return;
	const cwd = process.cwd();
	const matchers = new Map<Check, RegExp>();
	for (const check of plan.checks) {
		try {
			matchers.set(check, new RegExp(check.files));
		} catch {}
	}
	const applies = (check: Check, file: string) => matchers.get(check)?.test(file) ?? false;

	// The diagnostics of each file by tool, from the last time the tool ran in full: the baseline of the next edit.
	const baselines = new Map<string, Map<string, Finding[]>>();
	// The file and its text from before an edit, by tool call.
	const pending = new Map<string, { file: string; before: string | null }>();
	// The files the run changed with the edit and write tools, and whether every change was to comments only.
	const touched = new Map<string, { commentOnly: boolean }>();
	// Tools that are not installed, and tools noted as too slow, so that each is said once.
	const missing = new Set<string>();
	const noted = new Set<string>();
	// What the end-of-turn message told the agent already.
	const delivered = new Set<string>();
	let rounds = 0;
	let reportedError = false;

	const fileOf = (input: any): string | undefined => {
		if (!input || typeof input.path !== "string") return undefined;
		const rel = relative(cwd, path.resolve(cwd, input.path));
		return rel.startsWith("..") || path.isAbsolute(rel) ? undefined : rel;
	};
	const read = (file: string): string | null => {
		try {
			return fs.readFileSync(path.join(cwd, file), "utf8");
		} catch {
			return null;
		}
	};

	// Runs the checks of one file in parallel until `deadline`; the findings of a tool that ran in full are in `complete`.
	async function diagnose(file: string, text: string, deadline: number) {
		const findings: Finding[] = [];
		const complete = new Map<string, Finding[]>();
		const timings: Timing[] = [];
		const jobs: Promise<void>[] = [];
		const job = (phase: "syntax" | "lint", tool: string, check: Check, argv: string[]) => {
			if (missing.has(tool)) return;
			jobs.push(
				(async () => {
					const started = Date.now();
					const left = deadline - started;
					if (left <= 0) {
						timings.push({ phase, tool, ms: 0, skipped: "over budget", findings: 0 });
						return;
					}
					const ran = await run(argv, cwd, left);
					const ms = Date.now() - started;
					if (ran.missing) {
						missing.add(tool);
						timings.push({ phase, tool, ms, skipped: "not installed", findings: 0 });
					} else if (ran.timedOut) {
						timings.push({ phase, tool, ms, skipped: "over budget", findings: 0 });
					} else {
						const own = parse(ran.output, check, tool, cwd).filter((f) => f.file === file);
						findings.push(...own);
						complete.set(tool, own);
						timings.push({ phase, tool, ms, findings: own.length });
					}
				})(),
			);
		};
		if (plan!.syntax) {
			if (path.extname(file).toLowerCase() === ".json") {
				const started = Date.now();
				const own = jsonSyntax(file, text);
				findings.push(...own);
				complete.set("syntax", own);
				timings.push({ phase: "syntax", tool: "syntax", ms: Date.now() - started, findings: own.length });
			} else {
				const syntax = builtinSyntax(file);
				if (syntax && !plan!.checks.some((c) => c.syntax && applies(c, file))) job("syntax", "syntax", syntax.check, syntax.argv);
			}
		}
		for (const check of plan!.checks) {
			if (!applies(check, file)) continue;
			if (check.syntax) job("syntax", check.name, check, expand(check.syntax, { file }));
			if (check.edit) job("lint", check.name, check, expand(check.edit, { file }));
		}
		await Promise.all(jobs);
		return { findings, complete, timings };
	}

	pi.on("tool_call", async (event) => {
		try {
			if (event.toolName !== "edit" && event.toolName !== "write") return undefined;
			const file = fileOf(event.input);
			if (!file) return undefined;
			const before = read(file);
			pending.set(event.toolCallId, { file, before });
			// The diagnostics before the edit are the baseline, taken once for a file; later ones come from the last edit.
			if (before !== null && !baselines.has(file)) {
				const base = await diagnose(file, before, Date.now() + plan.budget_ms);
				baselines.set(file, base.complete);
			}
		} catch {}
		return undefined;
	});

	pi.on("tool_result", async (event) => {
		const edit = pending.get(event.toolCallId);
		pending.delete(event.toolCallId);
		try {
			if (!edit || event.isError) return undefined;
			const after = read(edit.file);
			if (after === null || after === edit.before) return undefined;
			const change = changeOf(edit.file, edit.before, after);
			const was = touched.get(edit.file);
			touched.set(edit.file, { commentOnly: (was?.commentOnly ?? true) && change.commentOnly });

			const now = await diagnose(edit.file, after, Date.now() + plan.budget_ms);
			for (const f of now.findings) f.touched = f.line >= change.first && f.line <= change.last;
			const baseline = baselines.get(edit.file) ?? new Map<string, Finding[]>();
			const fresh: Finding[] = [];
			const present: string[] = [];
			for (const [tool, list] of now.complete) {
				fresh.push(...newOnes(list, baseline.get(tool)));
				baseline.set(tool, list);
			}
			for (const list of baseline.values()) present.push(...list.map((f) => f.key));
			baselines.set(edit.file, baseline);

			let shown = fresh;
			try {
				const answer = await ask("report", {
					tool_call_id: event.toolCallId,
					file: edit.file,
					edit: { tool: event.toolName, first_line: change.first, last_line: change.last, excerpt: change.excerpt.slice(0, 4000) },
					max_lines: plan.max_lines,
					findings: fresh,
					present,
					checks: now.timings,
				});
				if (!Array.isArray(answer.shown)) throw new Error("no shown in the answer");
				shown = answer.shown.map((i: number) => fresh[i]).filter((f: Finding | undefined) => f !== undefined);
			} catch {
				// The server cannot be asked: the safe side is to show the findings.
				shown = fresh;
			}
			const lines = shown.map(describe);
			for (const t of now.timings) {
				if (t.skipped === "over budget" && !noted.has(t.tool)) {
					noted.add(t.tool);
					lines.push(`${t.tool} was skipped: it took longer than the ${plan.budget_ms} ms budget for one edit.`);
				}
			}
			if (lines.length === 0) return undefined;
			const found = shown.length;
			const head = found > 0 ? `Moebius lens: ${found} new problem${found === 1 ? "" : "s"} after this edit of ${edit.file}:` : "Moebius lens:";
			const shownLines = lines.slice(0, Math.max(plan.max_lines, found));
			return { content: [...event.content, { type: "text" as const, text: `\n${head}\n${shownLines.join("\n")}` }] };
		} catch (error) {
			if (!reportedError) {
				reportedError = true;
				console.error(`moebius-lens: ${error}`);
			}
			return undefined;
		}
	});

	// The files the run changed, with the edit tools or by other means (the shell).
	async function changedFiles(): Promise<{ path: string; comment_only: boolean }[]> {
		const names = new Set<string>(touched.keys());
		for (const argv of [["git", "diff", "--name-only", "HEAD"], ["git", "ls-files", "--others", "--exclude-standard"]]) {
			const ran = await run(argv, cwd, 10_000);
			for (const line of ran.output.split("\n")) {
				const name = line.trim();
				if (name !== "" && !name.startsWith(".moebius/") && fs.existsSync(path.join(cwd, name))) names.add(name);
			}
		}
		return [...names].sort().map((name) => ({ path: name, comment_only: touched.get(name)?.commentOnly ?? false }));
	}

	// Before the run settles: the slow check, once for what the run changed, delivered as one message.
	pi.on("agent_before_settle", async () => {
		try {
			if (rounds >= DEEP_ROUNDS) return undefined;
			const files = await changedFiles();
			const checks = plan.checks.filter((c) => c.deep && !missing.has(`deep:${c.name}`) && files.some((f) => applies(c, f.path)));
			if (checks.length === 0) return undefined;
			rounds += 1;
			let go = true;
			let judgement: number | null = null;
			try {
				const answer = await ask("deep", { files, round: rounds, tools: checks.map((c) => c.name) });
				go = answer.run !== false;
				judgement = typeof answer.judgement_id === "number" ? answer.judgement_id : null;
			} catch {
				// The server cannot be asked: the safe side is to run the check.
			}
			if (!go) return undefined;
			const timings: Timing[] = [];
			const fresh: Finding[] = [];
			const names = new Set(files.map((f) => f.path));
			await Promise.all(
				checks.map(async (check) => {
					const mine = files.filter((f) => applies(check, f.path)).map((f) => f.path);
					const dirs = [...new Set(mine.map((f) => `./${path.dirname(f)}`))];
					const started = Date.now();
					const ran = await run(expand(check.deep!, { files: mine, dirs }), cwd, plan.deep_timeout_ms);
					const ms = Date.now() - started;
					if (ran.missing) {
						missing.add(`deep:${check.name}`);
						timings.push({ phase: "deep", tool: check.name, ms, skipped: "not installed", findings: 0 });
						return;
					}
					if (ran.timedOut) {
						timings.push({ phase: "deep", tool: check.name, ms, skipped: "over budget", findings: 0 });
						return;
					}
					const all = parse(ran.output, check, check.name, cwd).filter((f) => names.has(f.file));
					const news = all.filter((f) => !delivered.has(`${f.file}|${f.key}`));
					for (const f of news) delivered.add(`${f.file}|${f.key}`);
					fresh.push(...news);
					timings.push({ phase: "deep", tool: check.name, ms, findings: all.length, shown: news.length });
				}),
			);
			try {
				await ask("deep-result", { judgement_id: judgement, new_findings: fresh.length, checks: timings });
			} catch {}
			if (fresh.length === 0) return undefined;
			fresh.sort((x, y) => Number(y.severity === "error") - Number(x.severity === "error") || x.file.localeCompare(y.file) || x.line - y.line);
			const lines = fresh.slice(0, plan.max_lines).map(describe);
			const more = fresh.length > lines.length ? `\n(${fresh.length - lines.length} more)` : "";
			const text = `Moebius lens: the type check found ${fresh.length} new problem${fresh.length === 1 ? "" : "s"} in files you changed:\n${lines.join("\n")}${more}\nFix them, or say why they can stay.`;
			pi.sendUserMessage(text, { deliverAs: "followUp" });
		} catch (error) {
			console.error(`moebius-lens: ${error}`);
		}
		return undefined;
	});

	// A plan that could not be read is said once, in the run's events.
	if (plan.error) {
		void ask("report", {
			file: ".moebius/lint.toml",
			edit: { tool: "lens", first_line: 1, last_line: 1, excerpt: "" },
			max_lines: plan.max_lines,
			findings: [],
			present: [],
			checks: [{ phase: "lint", tool: "lint.toml", ms: 0, skipped: plan.error, findings: 0 }],
		}).catch(() => {});
	}
}
