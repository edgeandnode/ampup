import { useEffect, useRef, useState } from "react";
import { AnimatePresence, LazyMotion, domMax, m } from "motion/react";
import { Button, CodeBlock } from "@edgeandnode/eds-react";
import { ArrowSquareOutIcon } from "@edgeandnode/eds-react/icons";
import { NetworkIcon } from "@web3icons/react/dynamic";
import { LINKS } from "@/data/constants";
import { ampClient } from "@/lib/amp-client";
import type { Network, StreamRow } from "@/lib/amp-api";
import { NETWORK_ICON, useHeads } from "@/lib/use-heads";

export default function Live() {
  const heads = useHeads();
  const [network, setNetwork] = useState<Network>("mainnet");
  const [rows, setRows] = useState<StreamRow[]>([]);
  const [meta, setMeta] = useState<{ batch: StreamRow[]; prev: string }>();
  const last = useRef("");
  const t = TABLES[network];

  useEffect(() => {
    setRows([]);
    setMeta(undefined);
    last.current = "";
    return ampClient.subscribeStream(network, (batch) => {
      setMeta({ batch, prev: last.current || batch[0]!.hash });
      last.current = batch[batch.length - 1]!.hash;
      setRows((prev) => [...batch.slice().reverse(), ...prev].slice(0, 8));
    });
  }, [network]);

  const first = meta?.batch[0];
  const end = meta?.batch[meta.batch.length - 1];

  return (
    <LazyMotion features={domMax}>
      <div className="flex flex-col border border-subtle">
        <div
          role="radiogroup"
          aria-label="Network"
          className="grid border-b border-subtle sm:grid-cols-3"
        >
          {heads.map((h) => {
            const on = h.network === network;
            return (
              <button
                key={h.network}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => setNetwork(h.network)}
                className={`flex flex-col gap-3 border-subtle p-5 text-start transition-colors max-sm:border-b sm:border-e sm:last:border-e-0 ${
                  on
                    ? "bg-canvas"
                    : "bg-subtle/40 text-muted hover:text-default"
                }`}
              >
                {on ? (
                  <span
                    aria-hidden
                    className="absolute inset-x-0 -top-px h-0.5 bg-brand-500"
                  />
                ) : null}
                <span className="flex items-center justify-between gap-3 text-14">
                  <span className="flex items-center gap-2 font-medium">
                    <NetworkIcon
                      name={NETWORK_ICON[h.network]}
                      variant={on ? "branded" : "mono"}
                      size={20}
                      aria-hidden
                    />
                    {h.label}
                  </span>
                  <span className="font-mono text-12 tabular-nums text-muted">
                    {h.lagMs} ms behind
                  </span>
                </span>
                <span className="flex flex-col gap-1">
                  <span className="text-12 text-muted">Amp head</span>
                  <span className="font-mono text-24 tabular-nums md:text-32">
                    {h.ampHead.toLocaleString("en-US")}
                  </span>
                </span>
                <span className="flex items-baseline justify-between gap-3 border-t border-subtle pt-3 text-12 text-muted">
                  Chain head
                  <span className="font-mono tabular-nums">
                    {h.chainHead.toLocaleString("en-US")}
                  </span>
                </span>
              </button>
            );
          })}
        </div>

        <div className="grid lg:grid-cols-[2fr_3fr]">
          <div className="flex flex-col gap-4 border-subtle p-4 max-lg:border-b md:p-6 lg:border-e">
            <CodeBlock language="sql" label="stream.sql">
              {`SELECT ${t.cols.map((c) => c.name).join(", ")}\nFROM "acme/${
                t.dataset
              }".${t.table}\nSETTINGS stream = true`}
            </CodeBlock>
            <CodeBlock language="json" label="app_metadata of the last batch">
              {first && end
                ? JSON.stringify(
                    {
                      ranges: [
                        {
                          network,
                          numbers: {
                            start: first.blockNum,
                            end: end.blockNum,
                          },
                          hash: short(end.hash),
                          prev_hash: short(meta!.prev),
                        },
                      ],
                      ranges_complete: true,
                    },
                    null,
                    2,
                  )
                : "{}"}
            </CodeBlock>
          </div>

          <div className="overflow-x-auto p-4 md:p-6">
            <div className="min-w-120 font-mono text-12 tabular-nums">
              <div className="grid grid-cols-[7rem_9rem_1fr_7rem] gap-4 border-b border-subtle pb-2">
                {t.cols.map((c) => (
                  <span key={c.name} className="flex flex-col gap-0.5">
                    <span className="text-default">{c.name}</span>
                    <span className="text-muted">{c.type}</span>
                  </span>
                ))}
              </div>
              <AnimatePresence initial={false} mode="popLayout">
                {rows.map((r) => (
                  <m.div
                    key={r.blockNum}
                    layout
                    initial={{
                      opacity: 0,
                      backgroundColor: "var(--background-color-brand-subtle)",
                    }}
                    animate={{ opacity: 1, backgroundColor: "rgba(0,0,0,0)" }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.8 }}
                    className="grid grid-cols-[7rem_9rem_1fr_7rem] gap-4 border-b border-subtle py-2"
                  >
                    {t.cells(r).map((v, i) => (
                      <span key={i} className="truncate">
                        {v}
                      </span>
                    ))}
                  </m.div>
                ))}
              </AnimatePresence>
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center justify-between gap-4 border-t border-subtle p-4 md:px-6">
          <p className="text-14 text-muted">
            Every batch carries the block range it covers and the hash that
            links it to the last one. A mismatch is a reorg, and the stream
            replays the range.
          </p>
          <Button
            variant="secondary"
            href={LINKS.contact}
            addonAfter={ArrowSquareOutIcon}
          >
            Talk to the team
          </Button>
        </div>
      </div>
    </LazyMotion>
  );
}

const short = (h: string) => `${h.slice(0, 10)}…${h.slice(-4)}`;
const time = (ms: number) => new Date(ms).toISOString().slice(11, 23);
const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const base58 = (h: string) =>
  Array.from(h.slice(2, 46), (c) => B58[parseInt(c, 16) * 3 + 1]).join("");

const EVM = (dataset: string) => ({
  dataset,
  table: "blocks",
  cols: [
    { name: "block_num", type: "UInt64" },
    { name: "timestamp", type: "Timestamp(ns)" },
    { name: "hash", type: "FixedSizeBinary(32)" },
    { name: "gas_used", type: "UInt64" },
  ],
  cells: (r: StreamRow) => [
    r.blockNum.toLocaleString("en-US"),
    time(r.ts),
    short(r.hash),
    r.gasUsed.toLocaleString("en-US"),
  ],
});

const TABLES: Record<Network, ReturnType<typeof EVM>> = {
  mainnet: EVM("eth_mainnet"),
  base: EVM("base"),
  "solana-mainnet": {
    dataset: "solana_mainnet",
    table: "block_headers",
    cols: [
      { name: "slot", type: "UInt64" },
      { name: "block_time", type: "Int64" },
      { name: "block_hash", type: "Utf8" },
      { name: "parent_slot", type: "UInt64" },
    ],
    cells: (r) => [
      r.blockNum.toLocaleString("en-US"),
      String(Math.floor(r.ts / 1000)),
      `${base58(r.hash).slice(0, 12)}…`,
      (r.blockNum - 1).toLocaleString("en-US"),
    ],
  },
};
