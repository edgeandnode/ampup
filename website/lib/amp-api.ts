/** The data the live section renders, behind one interface. `createMockClient`
 *  fabricates plausible values; a real client implements the same interface
 *  against the Amp Cloud API and is swapped in at one call site (Live.tsx). */

export type Network = "mainnet" | "base" | "solana-mainnet";

export interface ChainHead {
  network: Network;
  label: string;
  /** Latest block or slot on the chain. */
  chainHead: number;
  /** Latest block or slot Amp has materialized. */
  ampHead: number;
  /** Milliseconds between the chain head and Amp's head. */
  lagMs: number;
  /** Seconds per block or slot, for the ticker. */
  blockTimeS: number;
}

export interface StreamRow {
  network: Network;
  blockNum: number;
  hash: string;
  txCount: number;
  gasUsed: number;
  ts: number;
  /** The cursor field on the last row of a microbatch. */
  rangesComplete: boolean;
}

export interface DatasetSummary {
  name: string;
  version: string;
  kind: string;
  network: Network;
  tables: number;
  sizeTb: number;
  lagMs: number;
  status: "following" | "backfilling";
}

export interface SeriesPoint {
  day: string;
  value: number;
}

export interface AmpClient {
  heads(): ChainHead[];
  subscribeHeads(cb: (heads: ChainHead[]) => void): () => void;
  subscribeStream(network: Network, cb: (rows: StreamRow[]) => void): () => void;
  datasets(): DatasetSummary[];
  series(network: Network, days: number): SeriesPoint[];
}

/* ------------------------------ mock ------------------------------------ */

function rng(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

function hex(n: number, r: () => number) {
  let out = "0x";
  for (let i = 0; i < n; i++) out += Math.floor(r() * 16).toString(16);
  return out;
}

const BASELINE: ChainHead[] = [
  { network: "mainnet", label: "Ethereum", chainHead: 25_931_402, ampHead: 25_931_402, lagMs: 740, blockTimeS: 12 },
  { network: "base", label: "Base", chainHead: 51_204_118, ampHead: 51_204_117, lagMs: 810, blockTimeS: 2 },
  { network: "solana-mainnet", label: "Solana", chainHead: 445_218_930, ampHead: 445_218_928, lagMs: 690, blockTimeS: 0.4 },
];

export function createMockClient(): AmpClient {
  const r = rng(7);
  const start = Date.now();
  const heads = () =>
    BASELINE.map((h) => {
      const elapsed = (Date.now() - start) / 1000;
      const advanced = Math.floor(elapsed / h.blockTimeS);
      const jitter = Math.floor(Math.sin(elapsed / 3 + h.blockTimeS) * 90);
      return { ...h, chainHead: h.chainHead + advanced, ampHead: h.ampHead + advanced, lagMs: h.lagMs + jitter };
    });

  return {
    heads,
    subscribeHeads(cb) {
      cb(heads());
      const id = setInterval(() => cb(heads()), 400);
      return () => clearInterval(id);
    },
    subscribeStream(network, cb) {
      const head = BASELINE.find((h) => h.network === network)!;
      let next = head.chainHead - 12;
      const batch = () => {
        const size = network === "solana-mainnet" ? 5 : network === "base" ? 3 : 1;
        const rows: StreamRow[] = [];
        for (let i = 0; i < size; i++) {
          next += 1;
          rows.push({
            network,
            blockNum: next,
            hash: hex(64, r),
            txCount: network === "solana-mainnet" ? 900 + Math.floor(r() * 700) : network === "base" ? 200 + Math.floor(r() * 220) : 120 + Math.floor(r() * 160),
            gasUsed: network === "solana-mainnet" ? 0 : 12_000_000 + Math.floor(r() * 18_000_000),
            ts: Date.now(),
            rangesComplete: i === size - 1,
          });
        }
        cb(rows);
      };
      batch();
      const id = setInterval(batch, Math.max(1200, head.blockTimeS * 1000 * (network === "solana-mainnet" ? 5 : network === "base" ? 3 : 1)));
      return () => clearInterval(id);
    },
    datasets() {
      return [
        { name: "acme/eth_mainnet", version: "1.2.0", kind: "evm-rpc", network: "mainnet", tables: 3, sizeTb: 4.1, lagMs: 740, status: "following" },
        { name: "acme/base", version: "1.0.3", kind: "evm-rpc", network: "base", tables: 3, sizeTb: 2.7, lagMs: 810, status: "following" },
        { name: "acme/solana_mainnet", version: "2.0.0", kind: "solana", network: "solana-mainnet", tables: 5, sizeTb: 159.2, lagMs: 690, status: "following" },
        { name: "acme/erc20_transfers", version: "1.4.1", kind: "derived", network: "mainnet", tables: 1, sizeTb: 0.9, lagMs: 2_100, status: "following" },
        { name: "acme/usdc_volume_daily", version: "0.3.0", kind: "derived", network: "mainnet", tables: 1, sizeTb: 0.01, lagMs: 0, status: "backfilling" },
      ];
    },
    series(network, days) {
      const s = rng(network === "mainnet" ? 11 : network === "base" ? 13 : 17);
      const base = network === "mainnet" ? 1_520_000 : network === "base" ? 9_800_000 : 310_000_000;
      const out: SeriesPoint[] = [];
      const today = new Date();
      for (let i = days - 1; i >= 0; i--) {
        const d = new Date(today);
        d.setDate(today.getDate() - i);
        const weekday = d.getDay();
        const weekend = weekday === 0 || weekday === 6 ? 0.86 : 1;
        const trend = 1 + (days - 1 - i) * 0.004;
        out.push({ day: d.toISOString().slice(0, 10), value: Math.round(base * weekend * trend * (0.94 + s() * 0.12)) });
      }
      return out;
    },
  };
}
