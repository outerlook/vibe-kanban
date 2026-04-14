import type { INodePropertyOptions } from 'n8n-workflow';

import type {
  VkBaseCodingAgent,
  VkExecutorConfigs,
  VkExecutorProfileId,
} from './vk-contracts';

function toDisplayLabel(value: string): string {
  return value
    .split('_')
    .map((segment) =>
      segment.length === 0
        ? segment
        : `${segment[0].toUpperCase()}${segment.slice(1).toLowerCase()}`,
    )
    .join(' ');
}

export function parseExecutorConfigs(raw: string): VkExecutorConfigs {
  const parsed = JSON.parse(raw) as VkExecutorConfigs;

  if (!parsed || typeof parsed !== 'object' || !parsed.executors) {
    throw new Error('VK profiles payload is missing the executors catalog');
  }

  return parsed;
}

export function buildExecutorProfileId(
  executor: string,
  variant: string,
): VkExecutorProfileId | null {
  if (!executor) {
    return null;
  }

  return {
    executor: executor as VkBaseCodingAgent,
    variant: variant || null,
  };
}

export function toExecutorOptions(
  profiles: VkExecutorConfigs,
): INodePropertyOptions[] {
  return Object.keys(profiles.executors)
    .sort()
    .map((executor) => ({
      name: toDisplayLabel(executor),
      value: executor,
    }));
}

export function toExecutorVariantOptions(
  profiles: VkExecutorConfigs,
  executor: string,
): INodePropertyOptions[] {
  const options: INodePropertyOptions[] = [
    {
      name: 'Default',
      value: '',
    },
  ];

  if (!executor) {
    return options;
  }

  const config = profiles.executors[executor as VkBaseCodingAgent];
  if (!config) {
    return options;
  }

  const variants = Object.keys(config)
    .filter((variant) => variant !== 'DEFAULT')
    .sort();

  return options.concat(
    variants.map((variant) => ({
      name: toDisplayLabel(variant),
      value: variant,
    })),
  );
}
