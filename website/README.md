# ampup

Landing page for [Amp](https://github.com/edgeandnode/amp), aimed at data engineers and data scientists.

Vite 8, React 19, React Router 7, Tailwind 4 (via `@tailwindcss/vite`), Motion, pnpm. A static single-page app: `index.html` carries the metadata and the Google Fonts link, `app/main.tsx` mounts the router. Code blocks are the EDS `CodeBlock` from `@edgeandnode/eds-react` (Shiki highlighting, copy button, dark and light themes).

## EDS

`app/globals.css` imports `@edgeandnode/eds-react` in place of `tailwindcss`, registers the source directories with `@source` (EDS turns off automatic scanning), and then layers the house tokens on top. `components/Providers.tsx` mounts the one `EDSProvider` with `theme="light"`. The `.eds` class in `globals.css` points EDS's `font-sans` and `font-mono` at Messina Sans and Geist Mono and maps `font-medium` to 600, the only heavier weight Messina has. `components/CodeBlock.tsx` is the one wrapper every page uses.

```sh
pnpm install
pnpm dev      # http://localhost:3000
pnpm build    # typecheck, then vite build into dist/
pnpm preview  # serve dist/ with the SPA fallback
```

## Routes

| Route | What | Code |
|---|---|---|
| `/` | The site. EDS tokens, Messina, light only. All new work happens here. | `components/Page.tsx` composes one file per section in `components/sections/`. Each section keeps its own copy as consts at the bottom of the file. |

## Copy rules

Copy follows `../amp-comms/DESIGN.md` §12 (no dashes, no flagged words, no "X, not Y", sentence case, one italic word per page at most). Claims come from `../amp-comms/content/FACTS.md` and the `../amp` repo docs. The build output can be linted with the same dash and word check `amp-comms/tools/build.py` runs.

Fonts: Messina Sans (licensed, in `public/fonts`, declared with `@font-face` in `globals.css`), Newsreader and Geist Mono (Google Fonts, linked from `index.html`).

Hosting: the site is a client-routed SPA, so the host must serve `index.html` for unknown paths (the usual SPA fallback). The security headers the Next config used to set now belong in the host's config.

## Live data

The preview and "Try the flow" sections on `/` render from `lib/amp-api.ts`. `createMockClient()` fabricates chain heads, a streaming feed, a 30-day series, and a dataset catalog behind the `AmpClient` interface. To go live, implement `AmpClient` against the real API and change the one `createMockClient()` call at the top of `components/Live.tsx`, and the `ampClient` singleton in `lib/amp-client.ts` that the figures read.
