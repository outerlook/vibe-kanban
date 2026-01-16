# Repository Guidelines

## Project Structure & Module Organization
- `crates/`: Rust workspace crates — `server` (API + bins), `db` (SQLx models/migrations), `executors`, `services`, `utils`, `deployment`, `local-deployment`, `remote`, `tauri-app` (Tauri desktop app).
- `crates/tauri-app/`: Tauri 2.x desktop application that embeds the server and frontend.
- `frontend/`: React + TypeScript app (Vite, Tailwind). Source in `frontend/src`.
- `frontend/src/components/dialogs`: Dialog components for the frontend.
- `remote-frontend/`: Remote deployment frontend.
- `shared/`: Generated TypeScript types (`shared/types.ts`). Do not edit directly.
- `assets/`, `dev_assets_seed/`, `dev_assets/`: Packaged and local dev assets.
- `scripts/`: Dev helpers (ports, DB preparation).
- `docs/`: Documentation files.

## Managing Shared Types Between Rust and TypeScript

ts-rs allows you to derive TypeScript types from Rust structs/enums. By annotating your Rust types with #[derive(TS)] and related macros, ts-rs will generate .ts declaration files for those types.
When making changes to the types, you can regenerate them using `pnpm run generate-types`
Do not manually edit shared/types.ts, instead edit crates/server/src/bin/generate_types.rs

## Build, Test, and Development Commands
- Install dependencies: `pnpm i`
- Install Tauri CLI: `cargo install tauri-cli@2.0`
- **Tauri dev (desktop app)**: `cargo tauri dev`
- **Tauri build (production)**: `cargo tauri build`
- Web dev (frontend + backend): `pnpm run dev`
- Backend (watch): `pnpm run backend:dev:watch`
- Frontend (dev): `pnpm run frontend:dev`
- Type checks: `pnpm run check` (frontend) and `pnpm run backend:check` (Rust cargo check)
- Rust tests: `cargo test --workspace`
- Generate TS types from Rust: `pnpm run generate-types` (or `generate-types:check` in CI)
- Prepare SQLx (offline): `pnpm run prepare-db`
- Prepare SQLx (remote package, postgres): `pnpm run remote:prepare-db`

## Coding Style & Naming Conventions
- Rust: `rustfmt` enforced (`rustfmt.toml`); group imports by crate; snake_case modules, PascalCase types.
- TypeScript/React: ESLint + Prettier (2 spaces, single quotes, 80 cols). PascalCase components, camelCase vars/functions, kebab-case file names where practical.
- Keep functions small, add `Debug`/`Serialize`/`Deserialize` where useful.

## Testing Guidelines
- Rust: prefer unit tests alongside code (`#[cfg(test)]`), run `cargo test --workspace`. Add tests for new logic and edge cases.
- Frontend: ensure `pnpm run check` and `pnpm run lint` pass. If adding runtime logic, include lightweight tests (e.g., Vitest) in the same directory.

## Dev Server Isolation Guidelines

When AI agents start dev servers for testing user projects, they must be aware of existing running instances to avoid conflicts:

### Existing Behavior (Enforced)
- The system **automatically stops** any existing dev servers for a project before starting a new one
- This prevents port conflicts and resource contention
- See `start_dev_server` endpoint in `crates/server/src/routes/task_attempts.rs`

### Agent Awareness
When an agent is about to start a dev server for testing:
1. **Check for existing dev servers** - The system will stop them automatically
2. **Use project-level dev_script** - Configure in project settings, not per-repo
3. **Expect automatic cleanup** - Previous dev servers are terminated before new ones start

### Port Management
- Dev servers use ports configured in the project's dev script (e.g., `npm run dev`)
- The system does not manage these ports - they're controlled by the user's application
- If port conflicts occur, the user should configure their dev script to use different ports

### Database Isolation
- Each task attempt runs in an isolated git worktree
- Dev servers run within the worktree context
- No database isolation concerns - the dev server uses the project's normal database

## Security & Config Tips
- Use `.env` for local overrides; never commit secrets. Key envs: `FRONTEND_PORT`, `BACKEND_PORT`, `HOST` 
- Dev ports and assets are managed by `scripts/setup-dev-environment.js`.
