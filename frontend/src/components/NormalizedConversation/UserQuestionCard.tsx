import { ChevronDown, MessageCircleQuestion } from 'lucide-react';
import type { QuestionData, QuestionAnswer } from 'shared/types';
import { cn } from '@/lib/utils';
import { useExpandable } from '@/stores/useExpandableStore';
import { AnsweredQuestionCard } from './AnsweredQuestionCard';

type StatusAppearance = 'default' | 'denied' | 'timed_out';

const APPEARANCE: Record<
  StatusAppearance,
  {
    border: string;
    headerBg: string;
    headerText: string;
    contentBg: string;
    contentText: string;
  }
> = {
  default: {
    border: 'border-blue-400/40',
    headerBg: 'bg-blue-50 dark:bg-blue-950/20',
    headerText: 'text-blue-700 dark:text-blue-300',
    contentBg: 'bg-blue-50 dark:bg-blue-950/20',
    contentText: 'text-blue-700 dark:text-blue-300',
  },
  denied: {
    border: 'border-red-400/40',
    headerBg: 'bg-red-50 dark:bg-red-950/20',
    headerText: 'text-red-700 dark:text-red-300',
    contentBg: 'bg-red-50 dark:bg-red-950/10',
    contentText: 'text-red-700 dark:text-red-300',
  },
  timed_out: {
    border: 'border-amber-400/40',
    headerBg: 'bg-amber-50 dark:bg-amber-950/20',
    headerText: 'text-amber-700 dark:text-amber-200',
    contentBg: 'bg-amber-50 dark:bg-amber-950/10',
    contentText: 'text-amber-700 dark:text-amber-200',
  },
};

interface UserQuestionCardProps {
  questions: QuestionData[];
  answers: QuestionAnswer[] | null;
  expansionKey: string;
  defaultExpanded?: boolean;
  statusAppearance?: StatusAppearance;
}

export function UserQuestionCard({
  questions,
  answers,
  expansionKey,
  defaultExpanded = false,
  statusAppearance = 'default',
}: UserQuestionCardProps) {
  const [expanded, toggle] = useExpandable(
    `user-question:${expansionKey}`,
    defaultExpanded
  );
  const tone = APPEARANCE[statusAppearance];

  return (
    <div className="inline-block w-full">
      <div
        className={cn('border w-full overflow-hidden rounded-sm', tone.border)}
      >
        <button
          onClick={(e: React.MouseEvent) => {
            e.preventDefault();
            toggle();
          }}
          className={cn(
            'w-full px-2 py-1.5 flex items-center gap-1.5 text-left border-b',
            tone.headerBg,
            tone.headerText,
            tone.border
          )}
        >
          <MessageCircleQuestion className="h-3 w-3" />
          <span className="min-w-0 truncate">
            <span className="font-semibold">User Question</span>
          </span>
          <div className="ml-auto flex items-center gap-2">
            <ChevronDown
              className={cn(
                'h-4 w-4 cursor-pointer transition-transform',
                expanded ? '' : '-rotate-90'
              )}
            />
          </div>
        </button>

        {expanded && (
          <div className={cn('px-3 py-2', tone.contentBg)}>
            <div className={cn('text-sm', tone.contentText)}>
              <AnsweredQuestionCard questions={questions} answers={answers} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default UserQuestionCard;
