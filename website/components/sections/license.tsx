import { Button, ButtonGroup } from "@edgeandnode/eds-react";
import { ArrowRightIcon } from "@edgeandnode/eds-react/icons";
import { Dots, Reveal } from "@/components";
import { LINKS } from "@/data/constants";

export function License() {
  return (
    <div className="eds-dark overflow-clip bg-brand-500">
      <Dots corner="tr" className="text-white/25" />
      <section
        id="license"
        className="mx-auto flex max-w-280 flex-col gap-16 border-x border-white/15 px-4 py-24 md:px-16 md:py-32"
      >
        <Reveal className="flex flex-col gap-10">
          <h2 className="max-w-240 text-48 font-medium text-balance text-white md:text-64">
            Running Amp in production needs a license.{" "}
            <span className="text-white/60">The team sets it up with you.</span>
          </h2>
          <ButtonGroup>
            <Button
              variant="secondary"
              size="large"
              href={LINKS.contact}
              addonAfter={ArrowRightIcon}
            >
              Talk to the team
            </Button>
          </ButtonGroup>
        </Reveal>
        <dl className="grid gap-8 md:grid-cols-3">
          {TERMS.map(([title, body]) => (
            <div
              key={title}
              className="flex flex-col gap-2 border-t border-white/25 pt-5"
            >
              <dt className="text-20 font-medium text-white">{title}</dt>
              <dd className="text-16 text-white/75">{body}</dd>
            </div>
          ))}
        </dl>
      </section>
    </div>
  );
}

const TERMS = [
  [
    "Flat license",
    "Unlimited queries. Nothing is metered per query or per dataset.",
  ],
  [
    "Your cloud or on-prem",
    "Parquet stays in your bucket and PostgreSQL stays on your network.",
  ],
  [
    "Or operated by Edge & Node",
    "The same binary, run for you by the team behind The Graph.",
  ],
];
