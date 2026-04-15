import type { ExecutionProcessRunReason } from 'shared/types';

// Process run reasons
export const PROCESS_RUN_REASONS = {
  SETUP_SCRIPT: 'setup_script' as ExecutionProcessRunReason,
  CLEANUP_SCRIPT: 'cleanup_script' as ExecutionProcessRunReason,
  CODING_AGENT: 'coding_agent' as ExecutionProcessRunReason,
  DEV_SERVER: 'dev_server' as ExecutionProcessRunReason,
  INTERNAL_AGENT: 'internal_agent' as ExecutionProcessRunReason,
} as const;

export const isCodingAgent = (
  runReason: ExecutionProcessRunReason
): boolean => {
  return runReason === PROCESS_RUN_REASONS.CODING_AGENT;
};

export const shouldShowInLogs = (
  runReason: ExecutionProcessRunReason
): boolean => {
  return (
    runReason !== PROCESS_RUN_REASONS.DEV_SERVER &&
    runReason !== PROCESS_RUN_REASONS.INTERNAL_AGENT
  );
};
