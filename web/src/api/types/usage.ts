/** `GET /v1/usage`; see docs/reference/desktop-api.md. */

export interface UsageTotals {
  tokens: number;
  calls: number;
  cached_share: number;
  model_ms: number;
  day_budget: number | null;
}

export interface UsageDay {
  date: string;
  tokens: number;
  model: string;
}

export interface ModelUsage {
  model: string;
  tokens: number;
  calls: number;
  input_tokens: number;
  cached_tokens: number;
}

export interface AgentUsage {
  agent: string;
  calls: number;
  tokens: number;
  input_tokens: number;
  cached_tokens: number;
  model_ms: number;
}

export interface ActivityUsage {
  activity: string;
  model_ms: number;
  calls: number;
  output_tokens: number;
}

export interface ToolWait {
  tool: string;
  calls: number;
  seconds: number;
}

export interface ClassUsage {
  class: string;
  model: string;
  tokens: number;
  calls: number;
}

export interface UsageReport {
  today: string;
  days_in_window: number;
  totals: UsageTotals;
  days: UsageDay[];
  models: ModelUsage[];
  agents: AgentUsage[];
  activities: ActivityUsage[];
  tool_waits: ToolWait[];
  classes?: ClassUsage[];
}
