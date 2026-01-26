import { useCallback, useId } from 'react';
import type { QuestionAnswer, QuestionData } from 'shared/types';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';

interface QuestionRendererProps {
  question: QuestionData;
  questionIndex: number;
  answer: QuestionAnswer | undefined;
  onAnswerChange: (answer: QuestionAnswer) => void;
}

function RadioButton({
  id,
  checked,
  onChange,
  disabled,
  className,
}: {
  id?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      id={id}
      className={cn(
        'h-4 w-4 shrink-0 rounded-full border border-primary-foreground ring-offset-background',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
        'disabled:cursor-not-allowed disabled:opacity-50',
        checked && 'border-[5px] border-primary',
        className
      )}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}

export function QuestionRenderer({
  question,
  questionIndex,
  answer,
  onAnswerChange,
}: QuestionRendererProps) {
  const baseId = useId();
  const selectedIndices = answer?.selected_indices ?? [];
  const otherText = answer?.other_text ?? '';
  const isOtherSelected = otherText.length > 0 || selectedIndices.includes(-1);

  const updateAnswer = useCallback(
    (newSelectedIndices: number[], newOtherText?: string) => {
      onAnswerChange({
        question_index: questionIndex,
        selected_indices: newSelectedIndices.filter((i) => i >= 0),
        other_text: newOtherText || undefined,
      });
    },
    [onAnswerChange, questionIndex]
  );

  const handleOptionToggle = useCallback(
    (optionIndex: number) => {
      if (question.multi_select) {
        const newIndices = selectedIndices.includes(optionIndex)
          ? selectedIndices.filter((i) => i !== optionIndex)
          : [...selectedIndices, optionIndex];
        updateAnswer(newIndices, otherText || undefined);
      } else {
        updateAnswer([optionIndex], undefined);
      }
    },
    [question.multi_select, selectedIndices, updateAnswer, otherText]
  );

  const handleOtherToggle = useCallback(() => {
    if (question.multi_select) {
      if (isOtherSelected) {
        updateAnswer(selectedIndices, undefined);
      } else {
        updateAnswer(selectedIndices, '');
      }
    } else {
      if (isOtherSelected) {
        updateAnswer([], undefined);
      } else {
        updateAnswer([], '');
      }
    }
  }, [question.multi_select, isOtherSelected, selectedIndices, updateAnswer]);

  const handleOtherTextChange = useCallback(
    (text: string) => {
      updateAnswer(
        question.multi_select ? selectedIndices : [],
        text || undefined
      );
    },
    [question.multi_select, selectedIndices, updateAnswer]
  );

  return (
    <div className="flex flex-col gap-3">
      {question.header && (
        <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          {question.header}
        </span>
      )}

      <p className="text-sm font-medium">{question.question}</p>

      <div
        className="flex flex-col gap-2"
        role={question.multi_select ? 'group' : 'radiogroup'}
        aria-label={question.question}
      >
        {question.options.map((option, optionIndex) => {
          const optionId = `${baseId}-option-${optionIndex}`;
          const isSelected = selectedIndices.includes(optionIndex);

          return (
            <div key={optionIndex} className="flex items-start gap-3">
              {question.multi_select ? (
                <Checkbox
                  id={optionId}
                  checked={isSelected}
                  onCheckedChange={() => handleOptionToggle(optionIndex)}
                  className="mt-0.5"
                />
              ) : (
                <RadioButton
                  id={optionId}
                  checked={isSelected}
                  onChange={() => handleOptionToggle(optionIndex)}
                  className="mt-0.5"
                />
              )}
              <div className="flex flex-col gap-0.5 flex-1 min-w-0">
                <Label
                  htmlFor={optionId}
                  className="text-sm cursor-pointer leading-tight"
                >
                  {option.label}
                </Label>
                {option.description && (
                  <span className="text-xs text-muted-foreground">
                    {option.description}
                  </span>
                )}
              </div>
            </div>
          );
        })}

        <div className="flex flex-col gap-2">
          <div className="flex items-start gap-3">
            {question.multi_select ? (
              <Checkbox
                id={`${baseId}-other`}
                checked={isOtherSelected}
                onCheckedChange={handleOtherToggle}
                className="mt-0.5"
              />
            ) : (
              <RadioButton
                id={`${baseId}-other`}
                checked={isOtherSelected}
                onChange={handleOtherToggle}
                className="mt-0.5"
              />
            )}
            <Label
              htmlFor={`${baseId}-other`}
              className="text-sm cursor-pointer leading-tight"
            >
              Other
            </Label>
          </div>

          {isOtherSelected && (
            <Input
              value={otherText}
              onChange={(e) => handleOtherTextChange(e.target.value)}
              placeholder="Please specify..."
              className="ml-7"
              autoFocus
            />
          )}
        </div>
      </div>
    </div>
  );
}

export default QuestionRenderer;
