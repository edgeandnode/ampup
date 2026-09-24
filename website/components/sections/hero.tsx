import { LINKS } from "@/data/constants";
import { Button } from "@edgeandnode/eds-react";
import { Dots, Reveal, Section } from "@/components";

export function Hero() {
  return (
    <Section id="hero" className="overflow-clip">
      <Dots corner="tr" />
      <div className="flex flex-col gap-16 px-4 md:px-16">
        <div className="flex max-w-200 flex-col gap-8">
          <p className="flex items-center gap-2 font-mono text-12 tracking-[0.18em] text-muted uppercase">
            <span aria-hidden className="size-1.5 rounded-full bg-brand-500" />
            Amp · the blockchain native database
          </p>
          <h1 className="hero-serif text-56 text-balance sm:text-64 lg:text-80">
            <span>
              Blockchain data that <em>behaves </em>{" "}
              <span className="text-muted">like the rest of your stack.</span>
            </span>
          </h1>
          <p className="max-w-160 text-18 md:text-20">
            Amp turns any RPC endpoint into Parquet you own and SQL you already
            know. Under a second behind the head, correct through reorgs, from
            one binary that runs on your machine, in your cloud, or operated by
            Edge & Node.
          </p>
          <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
            <Button variant="primary" size="large" href={LINKS.contact}>
              Talk to the team
            </Button>
            <p className="max-w-80 text-14 text-muted">
              Running Amp in production needs a license. The team sets it up
              with you.
            </p>
          </div>
        </div>

        <dl className="grid gap-6 border-t border-subtle pt-8 sm:grid-cols-2 lg:grid-cols-4">
          {STATS.items.map(({ label, description }) => (
            <div key={label} className="flex flex-col gap-1">
              <dt className="text-32 font-medium tabular-nums">{label}</dt>
              <dd className="text-14">{description}</dd>
            </div>
          ))}
        </dl>
      </div>
    </Section>
  );
}

const STATS = {
  items: [
    { label: "747 ms", description: "median freshness behind the chain head" },
    {
      label: "4,397×",
      description: "peak query speedup against BigQuery and RPC",
    },
    {
      label: "5×",
      description: "smaller at rest than BigQuery's public Solana dataset",
    },
    { label: "2", description: "services to run besides ampd" },
  ],
};
