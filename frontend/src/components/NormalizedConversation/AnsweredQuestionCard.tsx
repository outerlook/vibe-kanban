import { MessageCircleQuestion } from 'lucide-react';
import type { QuestionData, QuestionAnswer } from 'shared/types';
import { cn } from '@/lib/utils';

interface AnsweredQuestionCardProps {
  questions: QuestionData[];
  answers: QuestionAnswer[] | null;
}

export function AnsweredQuestionCard({
  questions,
  answers,
}: AnsweredQuestionCardProps) {
  const getAnswerForQuestion = (questionIndex: number): QuestionAnswer | undefined => {
    return answers?.find((a) => a.question_index === questionIndex);
  };

  const formatAnswer = (question: QuestionData, answer: QuestionAnswer | undefined): string => {
    if (!answer) return '(No answer)';

    const parts: string[] = [];

    for (const idx of answer.selected_indices) {
      if (idx >= 0 && idx < question.options.length) {
        parts.push(question.options[idx].label);
      }
    }

    if (answer.other_text) {
      parts.push(answer.other_text);
    }

    return parts.length > 0 ? parts.join(', ') : '(No answer)';
  };

  return (
    <div className="flex flex-col gap-2">
      {questions.map((question, index) => {
        const answer = getAnswerForQuestion(index);
        return (
          <div key={index} className="flex flex-col gap-1">
            <div className="flex items-start gap-2">
              <MessageCircleQuestion className="h-4 w-4 text-muted-foreground shrink-0 mt-0.5" />
              <div className="flex flex-col gap-0.5 min-w-0">
                {question.header && (
                  <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                    {question.header}
                  </span>
                )}
                <span className="text-sm">{question.question}</span>
                <span
                  className={cn(
                    'text-sm',
                    answer ? 'text-primary font-medium' : 'text-muted-foreground italic'
                  )}
                >
                  {formatAnswer(question, answer)}
                </span>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

export default AnsweredQuestionCard;
