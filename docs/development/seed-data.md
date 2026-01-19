# Seed Data

This guide explains how to generate seed data for local development.

## Overview

The seed data generator creates sample projects, tasks, and task groups to populate your local development environment. This is useful for:

- Testing features without manually creating data
- Ensuring a consistent development environment
- Demonstrating the application to others

## Generating Seed Data

Run the following command from the repository root:

```bash
pnpm run seed-db
```

This executes `cargo run --bin generate-seed`, which creates seed data in the `dev_assets/` folder.

## Resetting Your Development Environment

To start fresh with a clean database:

1. Stop any running development servers
2. Delete the `dev_assets/` folder:
   ```bash
   rm -rf dev_assets/
   ```
3. Restart the development server (`pnpm run dev`)

A blank database will be copied from `dev_assets_seed/` on startup.

## What Gets Generated

The seed data generator creates:

- **Projects** - Sample projects with different configurations
- **Tasks** - Tasks in various states (todo, in progress, done)
- **Task groups** - Logical groupings of related tasks
- **Dependencies** - Task dependency relationships

## Customisation

To modify the generated seed data, edit the generator source:

```
crates/server/src/bin/generate_seed.rs
```

After making changes, run `pnpm run seed-db` to regenerate the data.
