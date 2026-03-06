import type { PatchTypeWithKey } from '@/hooks/useConversationHistory';

/**
 * Converts conversation entries to a markdown string containing
 * only user and assistant messages in chronological order.
 */
export function conversationToMarkdown(entries: PatchTypeWithKey[]): string {
  const lines: string[] = [];

  for (const entry of entries) {
    if (entry.type !== 'NORMALIZED_ENTRY') continue;

    const { entry_type, content, timestamp } = entry.content;
    if (entry_type.type !== 'user_message' && entry_type.type !== 'assistant_message') continue;

    const role = entry_type.type === 'user_message' ? 'User' : 'Assistant';
    const ts = timestamp ? ` (${timestamp})` : '';

    lines.push(`## ${role}${ts}\n`);
    lines.push(content.trim());
    lines.push('');
  }

  return lines.join('\n').trim();
}
