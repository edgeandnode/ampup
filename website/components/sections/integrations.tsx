import { Tag } from "@edgeandnode/eds-react";
import { Reveal, Section } from "@/components";
import { INTEGRATIONS } from "@/data/integrations";

export function Integrations() {
  return (
    <Section
      id="integrations"
      className="grid gap-12 lg:grid-cols-12 lg:gap-16"
      theme="dark"
    >
      <Reveal className="flex flex-col gap-4 lg:col-span-4 px-4 md:px-16">
        <h2 className="text-32 font-medium text-balance md:text-40">
          Drops into the stack you already run
        </h2>
      </Reveal>
      <div className="flex flex-col gap-8 lg:col-span-8">
        {INTEGRATIONS.map((group) => (
          <div
            key={group.key}
            className="flex flex-col gap-3 border-t border-subtle pt-5"
          >
            <p className="font-mono text-12 tracking-[0.18em] text-muted uppercase">
              {GROUPS[group.key]}
            </p>
            <div className="flex flex-wrap gap-2">
              {group.items.map((it) => (
                <Tag
                  key={it.name}
                  variant="elevated"
                  size="medium"
                  addonBefore={
                    it.path ? (
                      <svg viewBox="0 0 24 24" aria-hidden className="size-4">
                        <path d={it.path} fill="currentColor" />
                      </svg>
                    ) : undefined
                  }
                >
                  {it.name}
                </Tag>
              ))}
            </div>
          </div>
        ))}
      </div>
    </Section>
  );
}

const GROUPS: Record<string, string> = {
  warehouses: "Data warehouses",
  lakes: "Data lakes and storage",
  bi: "BI and analytics",
  aiml: "AI and ML",
  streaming: "Streaming and serving",
  observability: "Observability",
};
