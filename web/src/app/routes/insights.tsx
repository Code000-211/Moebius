/** The routes of the app's area: Insight: Insights with its Analytics and Usage tabs, and Improve with its Retro, Lessons, Experiments and Decisions tabs. Pages are fetched when their route opens. */
import { lazy } from "react";

const AnalyticsPage = lazy(() =>
  import("../analytics/AnalyticsPage").then((all) => ({ default: all.AnalyticsPage })),
);
const UsagePage = lazy(() =>
  import("../usage/UsagePage").then((all) => ({ default: all.UsagePage })),
);
const RetroPage = lazy(() =>
  import("../retro/RetroPage").then((all) => ({ default: all.RetroPage })),
);
const LessonsPage = lazy(() =>
  import("../retro/LessonsPage").then((all) => ({ default: all.LessonsPage })),
);
const ExperimentsPage = lazy(() =>
  import("../experiments/ExperimentsPage").then((all) => ({ default: all.ExperimentsPage })),
);
const DecisionModelPage = lazy(() =>
  import("../decision/DecisionModelPage").then((all) => ({ default: all.DecisionModelPage })),
);

/** The routes of this area, as children of the shell's route. */
export const insightsRoutes = [
  { path: "insights", element: <AnalyticsPage /> },
  { path: "insights/usage", element: <UsagePage /> },
  { path: "improve", element: <RetroPage /> },
  { path: "improve/lessons", element: <LessonsPage /> },
  { path: "improve/experiments", element: <ExperimentsPage /> },
  { path: "improve/decisions", element: <DecisionModelPage /> },
];
