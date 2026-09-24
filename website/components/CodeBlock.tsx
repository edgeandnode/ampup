import { CodeBlock as EdsCodeBlock } from "@edgeandnode/eds-react";
import { cn } from "@/lib/cn";

export type Lang = "sh" | "sql" | "toml" | "json" | "py";

/** The EDS CodeBlock with this site's line-array interface. Square corners per the
 *  house system; Shiki highlighting and the copy button come from EDS. */
export default function CodeBlock({
  lines,
  lang,
  label,
  copy,
  className,
}: {
  lines: string[];
  lang: Lang;
  /** Header label; with one set, the copy button is always visible. */
  label?: string;
  /** Text to copy instead of the rendered code (for prompts and comments). */
  copy?: string;
  className?: string;
}) {
  return (
    <EdsCodeBlock
      language={lang}
      label={label}
      copy={copy ?? true}
      className={cn("rounded-none", className)}
    >
      {lines.join("\n")}
    </EdsCodeBlock>
  );
}
