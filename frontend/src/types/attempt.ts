import type { Workspace, Session, WorkspaceWithSession } from 'shared/types';

// Re-export from shared types for backwards compatibility
export type { WorkspaceWithSession };

/**
 * Create a WorkspaceWithSession from a Workspace and Session.
 */
export function createWorkspaceWithSession(
  workspace: Workspace,
  session: Session | null | undefined
): WorkspaceWithSession {
  return {
    ...workspace,
    session: session ?? null,
  };
}
