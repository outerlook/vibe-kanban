"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.normalizeReadOutput = normalizeReadOutput;
exports.normalizeActionOutput = normalizeActionOutput;
function normalizeReadOutput(resource, data) {
    return {
        resource,
        ...data,
        _meta: {
            resource,
            surface: 'orchestration-context',
        },
    };
}
function normalizeActionOutput(args) {
    const base = {
        resource: args.resource,
        operation: args.operation,
        ...args.identifiers,
    };
    if (args.operation === 'startFollowUp' && args.data) {
        const followUp = args.data;
        return {
            ...base,
            followUp,
            executionProcess: followUp.status === 'started'
                ? followUp.execution_process
                : null,
            queueEntry: followUp.status === 'queued'
                ? followUp.queue_entry
                : null,
        };
    }
    if (args.operation === 'queueFollowUp' ||
        args.operation === 'cancelQueuedFollowUp') {
        return {
            ...base,
            queue: args.data,
        };
    }
    if (args.operation === 'sendMessage' && args.data) {
        const message = args.data;
        return {
            ...base,
            message,
            userMessage: message.user_message,
            executionProcessId: message.execution_process_id,
        };
    }
    if (args.operation === 'answerApproval') {
        return {
            ...base,
            approvalStatus: args.data,
        };
    }
    if (args.operation === 'stopExecution') {
        return {
            ...base,
            stopped: true,
        };
    }
    return {
        ...base,
        result: args.data ?? null,
    };
}
//# sourceMappingURL=output.js.map