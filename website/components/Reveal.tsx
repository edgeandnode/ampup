import { cn } from "@/lib/cn";
import { m, LazyMotion, domAnimation, useReducedMotion } from "motion/react";

const EASE_OUT = [0.16, 1, 0.3, 1] as const;

/** Fade + rise on scroll into view. Content is visible without JS (SSR renders
 *  the resting state; motion only adds the entrance once hydrated). */
export function Reveal({
  children,
  delay = 0,
  className,
}: {
  children: React.ReactNode;
  delay?: number;
  className?: string;
}) {
  const reduce = useReducedMotion();
  if (reduce) return <div className={cn("min-w-0", className)}>{children}</div>;
  return (
    <LazyMotion features={domAnimation}>
      <m.div
        className={cn("min-w-0", className)}
        initial={{ opacity: 0, y: 16 }}
        whileInView={{ opacity: 1, y: 0 }}
        viewport={{ once: true, amount: 0.2 }}
        transition={{ duration: 0.6, delay, ease: EASE_OUT }}
      >
        {children}
      </m.div>
    </LazyMotion>
  );
}
