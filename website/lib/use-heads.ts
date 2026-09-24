import { useEffect, useState } from "react";
import { ampClient } from "./amp-client";
import type { ChainHead, Network } from "./amp-api";

export const NETWORK_ICON: Record<Network, string> = {
  mainnet: "ethereum",
  base: "base",
  "solana-mainnet": "solana",
};

export function useHeads() {
  const [heads, setHeads] = useState<ChainHead[]>(() => ampClient.heads());
  useEffect(() => ampClient.subscribeHeads(setHeads), []);
  return heads;
}
