import type {
  DispatchResult,
  MqttDispatchInput,
  ScheduledDispatchResult,
  ScheduledWorkflowDispatchInput,
} from '../runtime/contracts';
import type { RuntimeDependencies } from '../runtime/dependencies';

type OrchestrationWorkflowHandler = {
  workflowKey: string;
  matches(input: MqttDispatchInput): boolean | Promise<boolean>;
  run(input: MqttDispatchInput, deps: RuntimeDependencies): Promise<unknown>;
};

type ScheduledWorkflowHandler = {
  workflowKey: string;
  run(
    input: ScheduledWorkflowDispatchInput,
    deps: RuntimeDependencies,
  ): Promise<{ output?: unknown; nextCheckpoint?: unknown }>;
};

const orchestrationHandlers: OrchestrationWorkflowHandler[] = [];
const scheduledHandlers = new Map<string, ScheduledWorkflowHandler>();

export function registerOrchestrationWorkflowHandler(
  handler: OrchestrationWorkflowHandler,
): void {
  orchestrationHandlers.push(handler);
}

export function registerScheduledWorkflowHandler(
  handler: ScheduledWorkflowHandler,
): void {
  scheduledHandlers.set(handler.workflowKey, handler);
}

export async function dispatchOrchestrationEvent(
  input: MqttDispatchInput,
  deps: RuntimeDependencies,
): Promise<DispatchResult> {
  const matchedHandlers = [] as OrchestrationWorkflowHandler[];

  for (const handler of orchestrationHandlers) {
    if (await handler.matches(input)) {
      matchedHandlers.push(handler);
    }
  }

  if (matchedHandlers.length === 0) {
    return {
      disposition: 'skipped',
      handlerKeys: [],
      output: {
        reason: 'no_mqtt_handlers_registered',
        eventType: input.event.eventType,
      },
    };
  }

  const outputs = [] as unknown[];
  for (const handler of matchedHandlers) {
    outputs.push(await handler.run(input, deps));
  }

  return {
    disposition: 'handled',
    handlerKeys: matchedHandlers.map((handler) => handler.workflowKey),
    output: outputs as never,
  };
}

export async function dispatchScheduledWorkflow(
  input: ScheduledWorkflowDispatchInput,
  deps: RuntimeDependencies,
): Promise<ScheduledDispatchResult> {
  const handler = scheduledHandlers.get(input.workflowKey);
  if (!handler) {
    return {
      disposition: 'skipped',
      handlerKeys: [],
      output: {
        reason: 'no_scheduled_handler_registered',
        workflowKey: input.workflowKey,
      },
      nextCheckpoint: undefined,
    };
  }

  const result = await handler.run(input, deps);

  return {
    disposition: 'handled',
    handlerKeys: [handler.workflowKey],
    output: (result.output ?? null) as never,
    nextCheckpoint:
      result.nextCheckpoint === undefined ? undefined : (result.nextCheckpoint as never),
  };
}

registerScheduledWorkflowHandler({
  workflowKey: 'coderabbit/poll',
  async run(input) {
    return {
      output: {
        status: 'scaffold_ready',
        workflowKey: input.workflowKey,
        scheduledAt: input.scheduledAt,
        lastCheckpoint: input.checkpoint?.checkpoint ?? null,
      },
      nextCheckpoint: input.checkpoint?.checkpoint ?? null,
    };
  },
});
