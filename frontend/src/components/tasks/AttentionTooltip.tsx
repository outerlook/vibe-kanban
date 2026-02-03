import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { AlertTriangle, Check } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { reviewAttentionApi } from '@/lib/api';

interface AttentionTooltipProps {
  taskId: string;
  needsAttention: boolean;
}

export function AttentionTooltip({ taskId, needsAttention }: AttentionTooltipProps) {
  const [isOpen, setIsOpen] = useState(false);

  const { data, isLoading } = useQuery({
    queryKey: ['review-attention', taskId],
    queryFn: () => reviewAttentionApi.getLatestByTaskId(taskId),
    enabled: isOpen,
    staleTime: 30000,
  });

  const getTooltipContent = () => {
    if (isLoading) {
      return 'Loading...';
    }
    if (!data?.reasoning) {
      return 'No details available';
    }
    return data.reasoning;
  };

  return (
    <TooltipProvider>
      <Tooltip open={isOpen} onOpenChange={setIsOpen}>
        <TooltipTrigger asChild>
          <span>
            {needsAttention ? (
              <AlertTriangle className="h-4 w-4 text-amber-500" />
            ) : (
              <Check className="h-3.5 w-3.5 text-green-500" />
            )}
          </span>
        </TooltipTrigger>
        <TooltipContent side="top" className="max-w-xs">
          <p className="text-sm break-words whitespace-pre-wrap">
            {getTooltipContent()}
          </p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
