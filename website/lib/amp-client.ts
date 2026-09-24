import { createMockClient, type AmpClient } from "./amp-api";

/** One client for every live element on the page. Swap the factory for the
 *  real API client when it lands; nothing else changes. */
export const ampClient: AmpClient = createMockClient();
