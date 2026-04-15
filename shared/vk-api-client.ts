export type VkApiConfig = {
  baseUrl: string;
  authMode: 'none' | 'bearerToken' | 'customHeader';
  token?: string;
  headerName?: string;
  headerPrefix?: string;
};

export type VkApiEnvelope<T, E = unknown> = {
  success: boolean;
  data?: T;
  error_data?: E;
  message?: string;
};

export function normalizeVkApiBaseUrl(baseUrl: string): string {
  return `${baseUrl.replace(/\/+$/, '')}/api`;
}

export function buildVkApiHeaders(
  config: VkApiConfig,
  initHeaders?: Record<string, string>,
): Record<string, string> {
  const headers: Record<string, string> = {
    'content-type': 'application/json',
  };

  if (config.authMode === 'bearerToken' && config.token) {
    headers.Authorization = `${config.headerPrefix ?? 'Bearer '}${config.token}`;
  }

  if (config.authMode === 'customHeader' && config.token && config.headerName) {
    headers[config.headerName] = config.token;
  }

  if (!initHeaders) {
    return headers;
  }

  return { ...headers, ...initHeaders };
}

function parseEnvelopeBody<T>(text: string): VkApiEnvelope<T> | null {
  try {
    return JSON.parse(text) as VkApiEnvelope<T>;
  } catch {
    return null;
  }
}

export function createVkApiRequestClient(config: VkApiConfig) {
  return {
    async requestEnvelope<T, E = unknown>(
      path: string,
      init?: RequestInit,
    ): Promise<VkApiEnvelope<T, E>> {
      const response = await fetch(`${normalizeVkApiBaseUrl(config.baseUrl)}${path}`, {
        ...init,
        headers: buildVkApiHeaders(
          config,
          init?.headers as Record<string, string> | undefined,
        ),
      });

      const text = await response.text();
      const body = text.length > 0 ? parseEnvelopeBody<T>(text) : null;

      if (!response.ok) {
        throw new Error(body?.message ?? `VK request failed with HTTP ${response.status}`);
      }

      if (!body) {
        throw new Error('VK control plane returned an empty response');
      }

      return body as VkApiEnvelope<T, E>;
    },

    async request<T>(path: string, init?: RequestInit): Promise<T> {
      const body = await this.requestEnvelope<T>(path, init);

      if (!body.success || body.data === undefined) {
        throw new Error(body.message ?? 'VK control plane returned an unexpected response');
      }

      return body.data;
    },

    async requestRawJson<T>(path: string, init?: RequestInit): Promise<T> {
      const response = await fetch(`${normalizeVkApiBaseUrl(config.baseUrl)}${path}`, {
        ...init,
        headers: buildVkApiHeaders(
          config,
          init?.headers as Record<string, string> | undefined,
        ),
      });

      if (!response.ok) {
        throw new Error(`VK request failed with HTTP ${response.status}`);
      }

      return (await response.json()) as T;
    },
  };
}
