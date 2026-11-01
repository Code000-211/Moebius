/**
 * The entry of the app. Before anything renders, a `?login=` code in the address is taken out of it and sent to the
 * server (`auth/login.ts`); then the router, the query cache and the app start.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "react-router";
import { router } from "./app/routes";
import { startLogin } from "./auth/login";
import "./styles/tailwind.css";

void startLogin(window);

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, retry: false } },
});

const root = document.getElementById("root");
if (!root) throw new Error("index.html has no #root");
createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router()} />
    </QueryClientProvider>
  </StrictMode>,
);
