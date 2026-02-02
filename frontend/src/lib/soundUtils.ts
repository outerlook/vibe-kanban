import type { NotificationConfig, NotificationType, SoundFile } from 'shared/types';

/**
 * Play a sound by its identifier.
 * @param identifier - Sound identifier in format "bundled:SOUND_NAME" or "custom:filename.wav"
 */
export async function playSound(identifier: string): Promise<void> {
  const audio = new Audio(`/api/sounds/${identifier}`);
  try {
    await audio.play();
  } catch (err) {
    console.error('Failed to play sound:', err);
  }
}

/**
 * Get the sound identifier from a sound file and optional custom path.
 * @param soundFile - The bundled sound file enum value
 * @param customSoundPath - Optional custom sound filename (takes precedence if provided)
 * @returns Sound identifier in format "bundled:SOUND_NAME" or "custom:filename.wav"
 */
export function getSoundIdentifier(
  soundFile: SoundFile,
  customSoundPath: string | null
): string {
  if (customSoundPath) {
    return `custom:${customSoundPath}`;
  }
  return `bundled:${soundFile}`;
}

/**
 * Get the sound identifier for a given notification type based on config.
 * @param type - The notification type
 * @param config - The notification config containing sound settings
 * @returns Sound identifier string, or null if no sound should play for this type
 */
export function getSoundForNotificationType(
  type: NotificationType,
  config: NotificationConfig
): string | null {
  switch (type) {
    case 'agent_complete':
      return getSoundIdentifier(config.sound_file, config.custom_sound_path);
    case 'agent_error':
      return `bundled:${config.error_sound_file}`;
    default:
      return null;
  }
}
