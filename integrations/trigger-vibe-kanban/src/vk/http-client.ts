import { createVkApiRequestClient, type VkApiConfig } from '../../../../shared/vk-api-client';

export class VkHttpClient {
  private readonly client;

  constructor(config: VkApiConfig) {
    this.client = createVkApiRequestClient(config);
  }

  protected async requestEnvelope<T, E = unknown>(
    path: string,
    init?: RequestInit,
  ) {
    return this.client.requestEnvelope<T, E>(path, init);
  }

  protected async request<T>(path: string, init?: RequestInit): Promise<T> {
    return this.client.request<T>(path, init);
  }

  protected async requestRawJson<T>(path: string, init?: RequestInit): Promise<T> {
    return this.client.requestRawJson<T>(path, init);
  }
}
