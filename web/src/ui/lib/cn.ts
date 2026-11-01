// From BloopAI/vibe-kanban packages/ui/src/lib/cn.ts (Apache-2.0), changed: merges Tailwind classes with tailwind-merge, so a later class wins over an earlier one (the original called only clsx; Button did the merge itself).
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
