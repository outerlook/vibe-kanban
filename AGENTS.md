# Repository Guidelines

## Project Structure & Module Organization
- `crates/`: Rust workspace crates — `server` (API + bins), `db` (SQLx models/migrations), `executors`, `services`, `utils`, `deployment`, `local-deployment`, `remote`.
- `frontend/`: React + TypeScript app (Vite, Tailwind). Source in `frontend/src`.
- `frontend/src/components/dialogs`: Dialog components for the frontend.
- `remote-frontend/`: Remote deployment frontend.
- `shared/`: Generated TypeScript types (`shared/types.ts`). Do not edit directly.
- `assets/`, `dev_assets_seed/`, `dev_assets/`: Packaged and local dev assets.
- `npx-cli/`: Files published to the npm CLI package.
- `scripts/`: Dev helpers (ports, DB preparation).
- `docs/`: Documentation files.

## Managing Shared Types Between Rust and TypeScript

ts-rs allows you to derive TypeScript types from Rust structs/enums. By annotating your Rust types with #[derive(TS)] and related macros, ts-rs will generate .ts declaration files for those types.
When making changes to the types, you can regenerate them using `pnpm run generate-types`
Do not manually edit shared/types.ts, instead edit crates/server/src/bin/generate_types.rs

## Build, Test, and Development Commands
- Install: `pnpm i`
- Run dev (frontend + backend with ports auto-assigned): `pnpm run dev`
- Backend (watch): `pnpm run backend:dev:watch`
- Frontend (dev): `pnpm run frontend:dev`
- Type checks: `pnpm run check` (frontend) and `pnpm run backend:check` (Rust cargo check)
- Rust tests: `cargo test --workspace`
- Generate TS types from Rust: `pnpm run generate-types` (or `generate-types:check` in CI)
- Prepare SQLx (offline): `pnpm run prepare-db`
- Prepare SQLx (remote package, postgres): `pnpm run remote:prepare-db`
- Local NPX build: `pnpm run build:npx` then `pnpm pack` in `npx-cli/`

## Coding Style & Naming Conventions
- Rust: `rustfmt` enforced (`rustfmt.toml`); group imports by crate; snake_case modules, PascalCase types.
- TypeScript/React: ESLint + Prettier (2 spaces, single quotes, 80 cols). PascalCase components, camelCase vars/functions, kebab-case file names where practical.
- Keep functions small, add `Debug`/`Serialize`/`Deserialize` where useful.

## Testing Guidelines
- Rust: prefer unit tests alongside code (`#[cfg(test)]`), run `cargo test --workspace`. Add tests for new logic and edge cases.
- Frontend: ensure `pnpm run check` and `pnpm run lint` pass. If adding runtime logic, include lightweight tests (e.g., Vitest) in the same directory.

## E2E Testing Guidelines

When running e2e tests locally, ensure proper isolation from any running development instances to avoid conflicts and data mixing:

### Port Isolation
- **Always use explicit PORT environment variables** for e2e tests to guarantee unique ports
- Set `FRONTEND_PORT` and `BACKEND_PORT` to different values than your dev instance
- Example: `FRONTEND_PORT=4000 BACKEND_PORT=4001 pnpm run dev`
- The default dev ports are managed by `scripts/setup-dev-environment.js` and stored in `.dev-ports.json`

### Database Isolation
- Use a separate database file for e2e tests (e.g., `test_assets` instead of `dev_assets`)
- Never use the same `dev_assets` database for both dev and testing
- Set up a clean database state before each test run

### Running E2E Tests Alongside Dev
```bash
# Terminal 1: Your dev instance (uses auto-assigned ports from .dev-ports.json)
pnpm run dev

# Terminal 2: E2E tests with explicit ports to avoid conflicts
FRONTEND_PORT=4000 BACKEND_PORT=4001 TEST_DB_PATH=test_assets pnpm run test:e2e
```

### Cleanup
- Clear saved ports before e2e tests if needed: `node scripts/setup-dev-environment.js clear`
- This forces the e2e runner to allocate fresh ports

## Security & Config Tips
- Use `.env` for local overrides; never commit secrets. Key envs: `FRONTEND_PORT`, `BACKEND_PORT`, `HOST` 
- Dev ports and assets are managed by `scripts/setup-dev-environment.js`.
