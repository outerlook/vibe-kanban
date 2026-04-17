import { configure } from '@trigger.dev/sdk';

export function configureTriggerApiClient(): void {
  const accessToken = process.env.TRIGGER_SECRET_KEY?.trim();
  if (!accessToken) {
    throw new Error(
      'Trigger-backed execution requires TRIGGER_SECRET_KEY so tasks can be enqueued.',
    );
  }

  configure({
    accessToken,
    ...(process.env.TRIGGER_API_URL?.trim()
      ? { baseURL: process.env.TRIGGER_API_URL.trim() }
      : {}),
  });
}
