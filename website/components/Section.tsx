import { cn } from "@/lib/cn";
import { Fragment } from "react";

interface Props {
  id?: string;
  children: React.ReactNode;
  className?: string;
  theme?: "dark" | "light";
}

export function Section({
  id,
  children,
  className = "",
  theme = "light",
}: Props) {
  return (
    <div
      className={cn("bg-canvas", theme === "dark" ? "eds-dark" : "eds-light")}
    >
      <section
        id={id}
        data-theme={theme}
        className={cn(
          "py-20 md:py-28 mx-auto max-w-280 border-x border-subtle",
          theme === "dark" ? "eds-dark" : "eds-light",
          className,
        )}
      >
        {children}
      </section>
      <div aria-hidden className="border-t border-subtle" />
    </div>
  );
}
