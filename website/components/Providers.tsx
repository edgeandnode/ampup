import { EDSProvider } from "@edgeandnode/eds-react";

/** One EDSProvider per app. Light only. The archived versions at /v2 and /v3
 *  follow the OS through their own eds-system wrapper. */
export default function Providers({ children }: { children: React.ReactNode }) {
  return <EDSProvider theme="light">{children}</EDSProvider>;
}
