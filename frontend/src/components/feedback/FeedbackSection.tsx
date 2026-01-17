import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, MessageSquare } from 'lucide-react';
import { feedbackApi } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { FeedbackResponse } from 'shared/types';

interface FeedbackSectionProps {
  workspaceId: string;
  defaultOpen?: boolean;
  className?: string;
}

function formatTimeAgo(iso: string): string {
  const d = new Date(iso);
  const diffMs = Date.now() - d.getTime();
  const absSec = Math.round(Math.abs(diffMs) / 1000);

  const rtf =
    typeof Intl !== 'undefined' && typeof Intl.RelativeTimeFormat === 'function'
      ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
      : null;

  const to = (value: number, unit: Intl.RelativeTimeFormatUnit) =>
    rtf
      ? rtf.format(-value, unit)
      : `${value} ${unit}${value !== 1 ? 's' : ''} ago`;

  if (absSec < 60) return to(Math.round(absSec), 'second');
  const mins = Math.round(absSec / 60);
  if (mins < 60) return to(mins, 'minute');
  const hours = Math.round(mins / 60);
  if (hours < 24) return to(hours, 'hour');
  const days = Math.round(hours / 24);
  if (days < 30) return to(days, 'day');
  const months = Math.round(days / 30);
  if (months < 12) return to(months, 'month');
  const years = Math.round(months / 12);
  return to(years, 'year');
}

function FeedbackItem({ feedback }: { feedback: FeedbackResponse }) {
  const { t } = useTranslation('tasks');

  return (
    <div className="rounded-md border border-border bg-background p-3 space-y-2">
      <div className="text-xs text-muted-foreground">
        {t('feedback.collectedAt', { time: formatTimeAgo(feedback.collected_at) })}
      </div>
      {feedback.feedback ? (
        <pre className="text-sm whitespace-pre-wrap break-words font-mono bg-muted p-2 rounded overflow-x-auto">
          {JSON.stringify(feedback.feedback, null, 2)}
        </pre>
      ) : (
        <div className="text-sm text-muted-foreground italic">
          {t('feedback.noFeedback')}
        </div>
      )}
    </div>
  );
}

export function FeedbackSection({
  workspaceId,
  defaultOpen = false,
  className,
}: FeedbackSectionProps) {
  const { t } = useTranslation('tasks');
  const [isOpen, setIsOpen] = useState(defaultOpen);

  const { data: feedbackList = [], isLoading } = useQuery({
    queryKey: ['feedback', 'byWorkspace', workspaceId],
    queryFn: () => feedbackApi.getByWorkspaceId(workspaceId),
    enabled: !!workspaceId,
    staleTime: 30000,
  });

  if (isLoading) {
    return null;
  }

  if (feedbackList.length === 0) {
    return null;
  }

  const toggleOpen = () => setIsOpen(!isOpen);

  return (
    <div className={cn('border-t', className)}>
      <Button
        variant="ghost"
        className="w-full justify-start gap-2 px-4 py-2 h-auto font-medium rounded-none"
        onClick={toggleOpen}
      >
        {isOpen ? (
          <ChevronDown className="w-4 h-4 flex-shrink-0" />
        ) : (
          <ChevronRight className="w-4 h-4 flex-shrink-0" />
        )}
        <MessageSquare className="w-4 h-4 flex-shrink-0" />
        <span>{t('feedback.title')}</span>
        <span className="text-muted-foreground text-xs ml-auto">
          {feedbackList.length}
        </span>
      </Button>

      {isOpen && (
        <div className="px-4 pb-4 space-y-2">
          {feedbackList.map((fb) => (
            <FeedbackItem key={fb.id} feedback={fb} />
          ))}
        </div>
      )}
    </div>
  );
}
