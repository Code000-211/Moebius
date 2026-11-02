// From BloopAI/vibe-kanban packages/ui/src/components/Loader.tsx (Apache-2.0), changed to this app's design standard:
// the indicator is quiet and secondary rather than muted, and the message that comes with it is the size of body text,
// because an activity indicator carries real status.
import { CircleNotchIcon } from "@phosphor-icons/react";
import type React from "react";

interface LoaderProps {
  message?: string | React.ReactElement;
  size?: number;
  className?: string;
}

export const Loader: React.FC<LoaderProps> = ({ message, size = 24, className = "" }) => (
  <div
    className={`flex flex-col items-center justify-center gap-3 py-8 text-secondary ${className}`}
    role="status"
  >
    <CircleNotchIcon
      className="animate-spin motion-reduce:animate-none"
      style={{ width: size, height: size }}
      aria-hidden="true"
    />
    {!!message && <div className="text-center text-base">{message}</div>}
  </div>
);
