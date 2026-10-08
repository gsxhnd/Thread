import { cn } from "@/lib/utils";

/** 产品标记：一条连续的线，对应「灰线」。 */
export function BrandMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" aria-hidden className={cn("size-4", className)}>
      <path
        d="M3.5 15.5c2.8-7 4.6-7 7.2-3.2 2.4 3.5 4.2 3.5 6.6-1.2 1.2-2.4 2.2-3.2 3.2-2.6"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
      />
    </svg>
  );
}
