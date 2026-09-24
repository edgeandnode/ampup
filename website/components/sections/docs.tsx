import { Link, TabSet } from "@edgeandnode/eds-react";
import { Section } from "@/components";

export function Docs() {
  return (
    <Section id="docs" className="flex flex-col gap-10">
      <h2 className="text-32 font-medium md:text-40">Docs and references</h2>
      <TabSet defaultValue={TABS[0]!.value} className="flex flex-col gap-8">
        <TabSet.Tabs>
          {TABS.map((t) => (
            <TabSet.Tab key={t.value} value={t.value}>
              {t.label}
            </TabSet.Tab>
          ))}
        </TabSet.Tabs>
        <TabSet.Panels>
          {TABS.map((t) => (
            <TabSet.Panel
              key={t.value}
              value={t.value}
              className="grid gap-px border border-subtle bg-subtle md:grid-cols-3"
            >
              {t.items.map(([title, body]) => (
                <Link
                  key={title}
                  href="https://ampup.sh/docs"
                  variant="secondary"
                  className="flex flex-col gap-2 bg-canvas p-6"
                >
                  <span className="text-18 font-medium text-default">
                    {title}
                  </span>
                  <span className="text-14 text-muted">{body}</span>
                </Link>
              ))}
            </TabSet.Panel>
          ))}
        </TabSet.Panels>
      </TabSet>
    </Section>
  );
}

const TABS = [
  {
    value: "start",
    label: "Get started",
    items: [
      ["Quickstart", "Seven commands from nothing to a SQL result."],
      [
        "Operational modes",
        "solo, server, controller, worker, and what each one runs.",
      ],
      [
        "Configuration",
        "Storage, services, PostgreSQL, and environment overrides.",
      ],
    ],
  },
  {
    value: "reference",
    label: "Reference",
    items: [
      [
        "Table schemas",
        "Every column of every provider kind, with Arrow types.",
      ],
      ["Reorg protocol", "What app_metadata carries and how a client resumes."],
      [
        "Decode functions",
        "evm_decode_log, evm_decode_params, Borsh, SCALE, XDR.",
      ],
    ],
  },
  {
    value: "agents",
    label: "For agents",
    items: [
      [
        "Shipped skills",
        "amp-setup, amp-providers, amp-datasets, amp-query, and four more.",
      ],
      [
        "Retrieval docs",
        "Feature docs written to be loaded by an agent before it acts.",
      ],
      [
        "Deterministic queries",
        "Pinned versions return the same answer every time.",
      ],
    ],
  },
];
