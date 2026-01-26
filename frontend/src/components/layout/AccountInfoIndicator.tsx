import { Info } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useAccountInfo } from '@/hooks/useAccountInfo';
import { UsageLimitDisplay } from '@/components/layout/UsageLimitDisplay';
import { WeekProgressTracker } from '@/components/layout/WeekProgressTracker';

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
                {data.claude.usage && (
                  <div className="mt-2 pt-2 border-t space-y-2">
                    <UsageLimitDisplay
                      label="Session (5h)"
                      usedPercent={data.claude.usage.fiveHour.usedPercent}
                      resetsAt={data.claude.usage.fiveHour.resetsAt}
                    />
                    <UsageLimitDisplay
                      label="Weekly total"
                      usedPercent={data.claude.usage.sevenDay.usedPercent}
                      resetsAt={data.claude.usage.sevenDay.resetsAt}
                    />
                    {data.claude.usage.sevenDaySonnet && (
                      <UsageLimitDisplay
                        label="Weekly Sonnet"
                        usedPercent={data.claude.usage.sevenDaySonnet.usedPercent}
                        resetsAt={data.claude.usage.sevenDaySonnet.resetsAt}
                      />
                    )}
                    {data.claude.usage.sevenDayOpus && (
                      <UsageLimitDisplay
                        label="Weekly Opus"
                        usedPercent={data.claude.usage.sevenDayOpus.usedPercent}
                        resetsAt={data.claude.usage.sevenDayOpus.resetsAt}
                      />
                    )}
                    <WeekProgressTracker
                      resetsAt={data.claude.usage.sevenDay.resetsAt}
                    />
                  </div>
                )}
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
