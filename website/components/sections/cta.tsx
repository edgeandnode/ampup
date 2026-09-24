import { Button, ButtonGroup, CodeBlock } from "@edgeandnode/eds-react";
import { Dots } from "@/components";
import { INSTALL } from "@/data/constants";

export function CTA() {
  return (
    <section
      id="start"
      className="eds-dark overflow-clip bg-canvas text-default"
    >
      <Dots corner="br" />
      <div className="flex flex-col gap-12 py-20 md:py-28">
        <h2 className="max-w-200 text-32 font-medium text-balance md:text-40">
          Get started with Amp
        </h2>
        <div className="grid gap-10 md:grid-cols-3">
          {ITEMS.map((c, i) => (
            <div
              key={c.title}
              className="flex flex-col gap-4 border-t border-muted pt-6"
            >
              <span className="text-20 font-medium">{c.title}</span>
              <span className="text-14 text-muted">{c.body}</span>
              {i === 0 ? (
                <CodeBlock language="sh" copy={INSTALL}>
                  {INSTALL}
                </CodeBlock>
              ) : null}
              {c.cta ? (
                <ButtonGroup className="mt-auto">
                  <Button
                    variant={i === 2 ? "primary" : "secondary"}
                    href={c.href}
                  >
                    {c.cta}
                  </Button>
                </ButtonGroup>
              ) : null}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

const ITEMS: { title: string; body: string; cta?: string; href?: string }[] = [
  {
    title: "Install it",
    body: "One script installs ampup and the latest ampd.",
  },
  {
    title: "See it running",
    body: "Live heads, a streaming feed, and a dataset catalog.",
    cta: "Open the preview",
    href: "#live",
  },
  {
    title: "Talk to the team",
    body: "Questions, a demo, or an operated deployment.",
    cta: "Talk to the team",
    href: "https://edgeandnode.com/contact",
  },
];
