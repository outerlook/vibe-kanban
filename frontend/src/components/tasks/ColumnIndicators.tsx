import { motion } from 'framer-motion';
import { cn } from '@/lib/utils';
import { TASK_STATUSES } from '@/constants/taskStatuses';
import { statusLabelsShort, statusBoardColors } from '@/utils/statusLabels';

interface ColumnIndicatorsProps {
  currentIndex: number;
  onColumnSelect: (index: number) => void;
}

export function ColumnIndicators({
  currentIndex,
  onColumnSelect,
}: ColumnIndicatorsProps) {
  return (
    <div className="flex gap-2 px-4 py-2 overflow-x-auto scrollbar-hide">
      {TASK_STATUSES.map((status, index) => {
        const isActive = index === currentIndex;
        const colorVar = statusBoardColors[status];

        return (
          <button
            key={status}
            type="button"
            onClick={() => onColumnSelect(index)}
            className={cn(
              'relative shrink-0 px-3 py-1.5 rounded-full text-xs font-medium',
              'transition-colors duration-200',
              'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
              isActive
                ? 'text-white'
                : 'text-muted-foreground border border-border hover:border-muted-foreground/50'
            )}
            aria-current={isActive ? 'true' : undefined}
            aria-label={`${statusLabelsShort[status]} column`}
          >
            {isActive && (
              <motion.span
                layoutId="active-column-indicator"
                className="absolute inset-0 rounded-full"
                style={{ backgroundColor: `var(${colorVar})` }}
                transition={{ type: 'spring', bounce: 0.2, duration: 0.4 }}
              />
            )}
            <span className="relative z-10">{statusLabelsShort[status]}</span>
          </button>
        );
      })}
    </div>
  );
}
