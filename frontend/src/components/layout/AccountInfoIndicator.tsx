import { Info } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useAccountInfo } from '@/hooks/useAccountInfo';

export function AccountInfoIndicator() {
  const { data, isLoading } = useAccountInfo();

  if (isLoading) {
    return null;
  }

  const hasData = data?.claude || data?.codex;
  if (!hasData) {
    return null;
  }

  const formatDate = (dateStr: string | null | undefined) => {
    if (!dateStr) return null;
    try {
      return new Date(dateStr).toLocaleDateString();
    } catch {
      return dateStr;
    }
  };

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            className="flex items-center justify-center h-9 w-9 text-muted-foreground hover:text-foreground transition-colors"
            aria-label="Account information"
          >
            <Info className="h-4 w-4" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom" className="max-w-xs">
          <div className="space-y-2 text-sm">
            {data.claude && (
              <div>
                <div className="font-medium">Claude</div>
                <div className="text-muted-foreground">
                  {data.claude.subscriptionType}
                  {data.claude.rateLimitTier && (
                    <> ({data.claude.rateLimitTier})</>
                  )}
                </div>
              </div>
            )}
            {data.codex && (
              <div>
                <div className="font-medium">Codex</div>
                <div className="text-muted-foreground">
                  {data.codex.planType}
                  {data.codex.subscriptionActiveUntil && (
                    <>
                      {' '}
                      (expires {formatDate(data.codex.subscriptionActiveUntil)})
                    </>
                  )}
                </div>
              </div>
            )}
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
