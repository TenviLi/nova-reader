# Nova Reader — Frontend (apps/web)

## Build & Dev

```bash
# Development (localhost:5173, proxies /api → localhost:3000)
pnpm dev

# Type check
pnpm check

# Production build
pnpm build

# Preview production build
pnpm preview
```

## Stack

- **Framework**: SvelteKit 2.16 + Svelte 5.28 (Runes only)
- **Styling**: Tailwind CSS 4.1 + design tokens (ink-*, accent-*)
- **UI Components**: shadcn-svelte (bits-ui based, "nova" style)
- **State**: TanStack Query + class-based Svelte 5 rune stores
- **Icons**: `lucide-svelte` (app code), `@lucide/svelte` (shadcn internal)
- **Notifications**: `svelte-sonner`
- **i18n**: `paraglide-sveltekit` (Inlang)

## Conventions

### Svelte 5 (Runes only — NO legacy syntax)
- `$state()` / `$state.raw()` for reactivity
- `$derived()` for computed values
- `$effect()` for side effects
- `$props()` for component inputs — NEVER `export let`
- `$bindable()` for two-way binding props
- Class-based stores (see `stores/*.svelte.ts`)

### File Organization
```
src/
├── lib/
│   ├── components/   # Reusable UI (reader/, dashboard/, layout/, ui/)
│   ├── services/     # API client (api.ts)
│   ├── stores/       # Global state (auth.svelte.ts, reader.svelte.ts, settings.svelte.ts)
│   ├── queries/      # TanStack Query hooks
│   ├── types/        # TypeScript types (models.ts)
│   └── utils/        # Helpers (format.ts, shortcuts.ts)
├── routes/           # SvelteKit file-based routes
└── service-worker.ts # PWA offline support
```

### API Client (`$services/api`)
- All backend responses that return arrays are wrapped in `{ data: [...] }` — the `unwrapArray()` helper handles this
- Auth uses HTTP-only cookies (`nova_token`)
- Retries: max 3 with exponential backoff for 429/503/network errors
- Timeout: 30s per request

### Backend Response Format
Backend endpoints return **wrapped** responses for lists:
```json
{ "data": [...], "total": N }
```
The `ApiClient.unwrapArray()` method handles both formats (raw array or wrapped).

### Design System
- Color tokens: `ink-50`→`ink-950` (neutral), `accent-300`→`accent-600` (brand)
- Dark theme default (bg-ink-950, text-ink-50)
- Border: `border-ink-800/50`
- Cards: `bg-ink-900/20 border border-ink-800/50 rounded-xl`
- All animations use Tailwind utilities or CSS transitions

### shadcn-svelte Components
Located in `src/lib/components/ui/`. Install more:
```bash
pnpm dlx shadcn-svelte@latest add [component-name]
```
Available: button, badge, card, dialog, dropdown-menu, input, popover, scroll-area, select, separator, sheet, slider, tabs, tooltip, avatar

### Auth & Guards
- JWT stored as HTTP-only cookie
- Auth state in `$stores/auth.svelte`
- Route guards in `+layout.svelte` (redirects to /login if unauthenticated)
- Roles: Admin, Reader, Guest

### Reading System
- Reader store (`$stores/reader.svelte.ts`): manages book, chapters, content, scroll
- ReaderContent renders plain text with `\n` → `<p>` paragraph conversion
- Entity highlighting via inline `<span>` injection
- Immersive translation (4 modes: 原文/双语/译文/悬浮)
- Chapter progress saved to backend with debounced scroll tracking

### Testing
```bash
pnpm test        # Run vitest
pnpm test:e2e    # Playwright E2E (if configured)
```

## Critical Patterns

### DO
- Use `$state.raw()` for large readonly data (search results, graph nodes)
- Use TanStack Query for server data (`$lib/queries/`)
- Unwrap backend `{ data: [...] }` responses via `api.unwrapArray()`
- Use shadcn-svelte components for UI consistency
- i18n all user-facing strings via paraglide

### DON'T
- Never use `export let` — only `$props()`
- Never assume backend returns raw arrays — always use unwrapArray helpers
- Never cache API data in service worker (network-first for non-static)
- Never hardcode Chinese strings — use i18n messages
- Never add features without updating this file
