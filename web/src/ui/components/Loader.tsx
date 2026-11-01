// From BloopAI/vibe-kanban packages/ui/src/components/Loader.tsx (Apache-2.0), changed: the spinner is Phosphor's CircleNotch in place of lucide's Loader2 (one icon library); the colour name is the Deep Velvet muted text.
import { CircleNotchIcon } from "@phosphor-icons/react";
import type React from "react";

interface LoaderProps {
  message?: string | React.ReactElement;
  size?: number;
  className?: string;
}

export const Loader: React.FC<LoaderProps> = ({ message, size = 32, className = "" }) => (
  <div className={`flex flex-col items-center justify-center gap-2 ${className}`} role="status">
    <CircleNotchIcon className="animate-spin text-low" style={{ width: size, height: size }} />
    {!!message && <div className="text-center text-low">{message}</div>}
  </div>
);
