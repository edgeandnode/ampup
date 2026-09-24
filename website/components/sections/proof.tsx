import { Button, ButtonGroup, Table } from "@edgeandnode/eds-react";
import { ArrowRightIcon } from "@edgeandnode/eds-react/icons";
import { LINKS } from "@/data/constants";
import { Reveal, Section } from "@/components";

export function Proof() {
  return (
    <Section id="proof" className="flex flex-col gap-16 md:gap-24">
      <Reveal className="flex max-w-200 flex-col gap-4 px-4 md:px-16">
        <h2 className="text-32 font-medium text-balance md:text-40">
          Owning the engine pays off
        </h2>
        <p className="text-18 text-muted">
          We compared Amp with hosted vendors on storage, freshness, and cost.
        </p>
      </Reveal>
      {ROWS.map((r) => (
        <div
          key={r.title}
          className="grid gap-10 border-t border-subtle pt-12 lg:grid-cols-12 lg:gap-24 px-4 md:px-16"
        >
          <Reveal className="flex flex-col gap-5 lg:col-span-5">
            <h3 className="hero-serif text-40 text-balance md:text-48">
              {r.title}
            </h3>
            <p className="text-18 text-muted">{r.body}</p>
            <p className="text-12 text-muted font-mono">{r.note}</p>
            {r.cta ? (
              <ButtonGroup>
                <Button href={LINKS.contact} addonAfter={ArrowRightIcon}>
                  {r.cta}
                </Button>
              </ButtonGroup>
            ) : null}
          </Reveal>
          <Reveal delay={0.1} className="text-16 lg:col-span-7 px-4 md:px-16">
            {r.chart}
          </Reveal>
        </div>
      ))}
      <div className="border-t border-subtle pt-12">
        <Table>
          <Table.Header>
            <Table.HeaderCell />
            <Table.HeaderCell className="text-brand-800">Amp</Table.HeaderCell>
            <Table.HeaderCell>Hosted vendors</Table.HeaderCell>
          </Table.Header>
          <Table.Body>
            {COMPARE.map(([k, amp, hosted]) => (
              <Table.Row key={k}>
                <Table.Cell className="text-muted">{k}</Table.Cell>
                <Table.Cell className="font-medium text-brand-800">
                  {amp}
                </Table.Cell>
                <Table.Cell>{hosted}</Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table>
      </div>
    </Section>
  );
}

function Bars({
  rows,
  max,
}: {
  rows: { name: string; value: number; label: string; note?: string }[];
  max: number;
}) {
  return (
    <dl className="flex flex-col gap-5">
      {rows.map((r) => {
        const amp = r.name === "Amp";
        return (
          <div
            key={r.name}
            className="grid items-baseline gap-x-4 gap-y-1 sm:grid-cols-[6rem_1fr]"
          >
            <dt
              className={`text-14 font-medium ${amp ? "text-brand-800" : ""}`}
            >
              {r.name}
            </dt>
            <dd className="flex flex-col gap-1">
              <div className="flex items-center gap-3">
                <span
                  aria-hidden
                  className={`h-4 min-w-1 rounded-e-4 ${
                    amp ? "bg-brand-default" : "bg-foam-700"
                  }`}
                  style={{ width: `${Math.max(0.5, (r.value / max) * 100)}%` }}
                />
                <span
                  className={`shrink-0 text-14 ${
                    amp ? "font-medium text-brand-800" : ""
                  }`}
                >
                  {r.label}
                </span>
              </div>
              {r.note ? (
                <span className="text-12 text-muted">{r.note}</span>
              ) : null}
            </dd>
          </div>
        );
      })}
    </dl>
  );
}

// Breakeven and flat-line position, in percent of the plot box.
const BX = 15.5;
const FY = 63;
const CURVE = "M0 100 Q23 21 100 5.3";

const ROWS: {
  title: string;
  body: string;
  note: string;
  cta?: string;
  chart: React.ReactNode;
}[] = [
  {
    title: "5× smaller at rest",
    body: "Amp keeps raw blockchain data in compact encoded form at rest and decodes it at query time, instead of permanently expanding it into wide tables.",
    note: "Public Solana tables, same coverage window.",
    chart: (
      <Bars
        max={800}
        rows={[
          { name: "Amp", value: 159, label: "≈159 TB" },
          { name: "BigQuery", value: 800, label: "~800 TB" },
        ]}
      />
    ),
  },
  {
    title: "Under 3 minutes on derived Solana data",
    body: "How far behind the chain head a derived Solana table sits by the time it is queryable.",
    note: "Vendor figures from their published documentation.",
    chart: (
      <Bars
        max={420}
        rows={[
          {
            name: "Amp",
            value: 3,
            label: "under 3 minutes",
            note: "In the reported deployment",
          },
          {
            name: "Allium",
            value: 60,
            label: "1 hour",
            note: "Documented for most enriched historical tables",
          },
          {
            name: "Dune",
            value: 420,
            label: "6 to 7 hours",
            note: "Documented for Solana DEX trades and token transfers tables",
          },
        ]}
      />
    ),
  },
  {
    title: "Scale usage without scaling the bill",
    cta: "Price your workload",
    body: "Flat license fee. Unlimited usage on hardware you own. No vendor lock-in.",
    note: "",
    chart: (
      <figure className="flex flex-col gap-4">
        <figcaption className="flex flex-wrap gap-x-6 gap-y-2 text-12 text-muted">
          <span className="flex items-center gap-2">
            <span aria-hidden className="size-3 rounded-2 bg-space-600" />
            Pay per query or compute credits
          </span>
          <span className="flex items-center gap-2">
            <span aria-hidden className="size-3 rounded-2 bg-brand-default" />
            Amp, flat license
          </span>
        </figcaption>
        <div
          role="img"
          aria-label="Cost against number of queries: pay per query or compute credits keeps rising, Amp's flat license stays flat. Savings at scale past the breakeven point."
          className="flex flex-col gap-2"
        >
          <span className="text-12 text-muted">Cost</span>
          <div className="h-64 border-s border-b border-foam-500 md:h-80 lg:h-96">
            <svg
              viewBox="0 0 100 100"
              preserveAspectRatio="none"
              aria-hidden
              className="absolute inset-0 size-full overflow-visible"
            >
              <defs>
                <clipPath id="breakeven-clip">
                  <rect x={BX} y="0" width={100 - BX} height={FY} />
                </clipPath>
              </defs>
              <path
                d={`${CURVE} L100 100 L0 100 Z`}
                clipPath="url(#breakeven-clip)"
                className="fill-brand-500"
                opacity="0.14"
              />
              <path
                d={CURVE}
                fill="none"
                className="stroke-space-600"
                strokeWidth="2"
                strokeLinecap="round"
                vectorEffect="non-scaling-stroke"
              />
              <path
                d={`M0 ${FY} H100`}
                fill="none"
                className="stroke-brand-500"
                strokeWidth="2"
                strokeLinecap="round"
                vectorEffect="non-scaling-stroke"
              />
            </svg>
            <span
              aria-hidden
              className="absolute bottom-0 border-s border-dashed border-foam-800"
              style={{ left: `${BX}%`, top: `${FY}%` }}
            />
            <span
              aria-hidden
              className="absolute size-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-brand-500 bg-white"
              style={{ left: `${BX}%`, top: `${FY}%` }}
            />
            <span
              className="absolute bottom-2 text-12 text-muted"
              style={{ left: `calc(${BX}% + 12px)` }}
            >
              Breakeven
            </span>
            <span
              className="absolute end-4 text-14 font-medium text-brand-700"
              style={{ top: `${(FY + 5) / 2}%` }}
            >
              Savings at scale
            </span>
          </div>
          <span className="self-end text-12 text-muted">Number of queries</span>
        </div>
      </figure>
    ),
  },
];

const COMPARE = [
  ["Raw data freshness", "Under 1 second", "Seconds to over an hour"],
  ["Derived Solana freshness", "Under 3 minutes", "1 hour to 7 hours"],
  [
    "Storage at rest",
    "≈159 TB for all of Solana",
    "~800 TB for the same chain in BigQuery",
  ],
  ["Deployment", "Your cloud or on-prem", "Hosted only"],
  [
    "Billing",
    "Flat license, unlimited queries",
    "Per query, per dataset, or credits",
  ],
  ["Pipeline visibility", "The SQL of every pipeline, and the logs", "Limited"],
  ["Chain coverage", "Any EVM or SVM chain, Canton, others", "Fixed list"],
];
