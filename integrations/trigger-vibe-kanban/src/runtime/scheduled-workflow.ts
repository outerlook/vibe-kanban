import type {
  OrchestratorDispatcher,
  ScheduledDispatchResult,
} from './contracts';
import type { RuntimeDependencies } from './dependencies';

export async function runScheduledWorkflowOnce(
  args: {
    workflowKey: string;
    scopeKey: string;
    claimKey: string;
    scheduledAt?: string;
  },
  dependencies: RuntimeDependencies,
  dispatcher: OrchestratorDispatcher,
): Promise<ScheduledDispatchResult> {
  const scheduledAt = args.scheduledAt ?? new Date().toISOString();
  const claimResult = dependencies.stateStore.claimScheduledRun({
    claimKey: args.claimKey,
    workflowKey: args.workflowKey,
    scopeKey: args.scopeKey,
    metadata: {
      scheduledAt,
    },
  });

  if (!claimResult.claimed) {
    return {
      disposition: 'skipped',
      handlerKeys: [],
      output: {
        reason: 'duplicate_claim',
        claimKey: claimResult.record.claimKey,
      },
      nextCheckpoint:
        dependencies.stateStore.getCheckpoint(args.workflowKey, args.scopeKey)?.checkpoint,
    };
  }

  const checkpoint = dependencies.stateStore.getCheckpoint(
    args.workflowKey,
    args.scopeKey,
  );

  dependencies.stateStore.markClaimDispatched(claimResult.record.claimKey, {
    workflowKey: args.workflowKey,
    scheduledAt,
  });

  try {
    const result = await dispatcher.dispatchScheduledWorkflow({
      workflowKey: args.workflowKey,
      scopeKey: args.scopeKey,
      scheduledAt,
      claim: claimResult.record,
      checkpoint,
    });

    if (result.nextCheckpoint !== undefined) {
      dependencies.stateStore.putCheckpoint({
        workflowKey: args.workflowKey,
        scopeKey: args.scopeKey,
        claimKey: claimResult.record.claimKey,
        checkpoint: result.nextCheckpoint,
      });
    }

    dependencies.stateStore.markClaimCompleted(claimResult.record.claimKey, {
      workflowKey: args.workflowKey,
      disposition: result.disposition,
      handlerKeys: result.handlerKeys,
    });

    return result;
  } catch (error) {
    const message = error instanceof Error ? error.message : 'scheduled workflow failed';
    dependencies.stateStore.markClaimFailed(claimResult.record.claimKey, message);
    throw error;
  }
}
