import { useEffect, useMemo, useRef, useState } from 'react';
import { Virtuoso, VirtuosoHandle } from 'react-virtuoso';
import { AlertCircle, Search } from 'lucide-react';
import { useServerLogStream } from '@/hooks/useServerLogStream';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { cn } from '@/lib/utils';
import type { ServerLogEntry } from 'shared/types';

type LogLevel = 'ALL' | 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';

const LOG_LEVELS: LogLevel[] = ['ALL', 'TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR'];

const LEVEL_PRIORITY: Record<string, number> = {
  TRACE: 0,
  DEBUG: 1,
  INFO: 2,
  WARN: 3,
  ERROR: 4,
};

const getLevelStyles = (level: string): string => {
  switch (level.toUpperCase()) {
    case 'ERROR':
      return 'bg-destructive/20 text-destructive border-destructive/30';
    case 'WARN':
      return 'bg-yellow-500/20 text-yellow-600 dark:text-yellow-400 border-yellow-500/30';
    case 'INFO':
      return 'bg-blue-500/20 text-blue-600 dark:text-blue-400 border-blue-500/30';
    case 'DEBUG':
      return 'bg-muted text-muted-foreground border-muted-foreground/30';
    case 'TRACE':
      return 'bg-muted/50 text-muted-foreground/70 border-muted-foreground/20';
    default:
      return 'bg-muted text-muted-foreground border-muted-foreground/30';
  }
};

const formatTimestamp = (timestamp: string): string => {
  try {
    const date = new Date(timestamp);
    const hours = date.getHours().toString().padStart(2, '0');
    const minutes = date.getMinutes().toString().padStart(2, '0');
    const seconds = date.getSeconds().toString().padStart(2, '0');
    const millis = date.getMilliseconds().toString().padStart(3, '0');
    return `${hours}:${minutes}:${seconds}.${millis}`;
  } catch {
    return timestamp;
  }
};

interface LogEntryRowProps {
  entry: ServerLogEntry;
}

function LogEntryRow({ entry }: LogEntryRowProps) {
  return (
    <div className="flex items-start gap-3 px-4 py-1.5 font-mono text-sm hover:bg-muted/30">
      <span className="text-muted-foreground/70 whitespace-nowrap">
        {formatTimestamp(entry.timestamp)}
      </span>
      <span
        className={cn(
          'px-1.5 py-0.5 text-xs font-semibold rounded border whitespace-nowrap min-w-[4rem] text-center',
          getLevelStyles(entry.level)
        )}
      >
        {entry.level}
      </span>
      <span className="text-muted-foreground whitespace-nowrap truncate max-w-[200px]">
        {entry.target}
      </span>
      <span className="flex-1 break-words">{entry.message}</span>
    </div>
  );
}

export function ServerLogsViewer() {
  const { logs, error, isConnected } = useServerLogStream();
  const [levelFilter, setLevelFilter] = useState<LogLevel>('ALL');
  const [searchQuery, setSearchQuery] = useState('');

  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const didInitScroll = useRef(false);
  const prevLenRef = useRef(0);
  const [atBottom, setAtBottom] = useState(true);

  const filteredLogs = useMemo(() => {
    return logs.filter((log) => {
      // Level filter: show selected level and above
      if (levelFilter !== 'ALL') {
        const logPriority = LEVEL_PRIORITY[log.level.toUpperCase()] ?? 0;
        const filterPriority = LEVEL_PRIORITY[levelFilter] ?? 0;
        if (logPriority < filterPriority) {
          return false;
        }
      }

      // Text search filter
      if (searchQuery.trim()) {
        const query = searchQuery.toLowerCase();
        const matchesMessage = log.message.toLowerCase().includes(query);
        const matchesTarget = log.target.toLowerCase().includes(query);
        if (!matchesMessage && !matchesTarget) {
          return false;
        }
      }

      return true;
    });
  }, [logs, levelFilter, searchQuery]);

  // Initial jump to bottom once data appears
  useEffect(() => {
    if (!didInitScroll.current && filteredLogs.length > 0) {
      didInitScroll.current = true;
      requestAnimationFrame(() => {
        virtuosoRef.current?.scrollToIndex({
          index: filteredLogs.length - 1,
          align: 'end',
        });
      });
    }
  }, [filteredLogs.length]);

  // Handle large bursts while at bottom
  useEffect(() => {
    const prev = prevLenRef.current;
    const grewBy = filteredLogs.length - prev;
    prevLenRef.current = filteredLogs.length;

    const LARGE_BURST = 10;
    if (grewBy >= LARGE_BURST && atBottom && filteredLogs.length > 0) {
      requestAnimationFrame(() => {
        virtuosoRef.current?.scrollToIndex({
          index: filteredLogs.length - 1,
          align: 'end',
        });
      });
    }
  }, [filteredLogs.length, atBottom]);

  return (
    <div className="flex flex-col h-full">
      {/* Filter controls */}
      <div className="flex items-center gap-3 p-3 border-b">
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">Level:</span>
          <Select
            value={levelFilter}
            onValueChange={(v) => setLevelFilter(v as LogLevel)}
          >
            <SelectTrigger className="w-[120px] h-8">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {LOG_LEVELS.map((level) => (
                <SelectItem key={level} value={level}>
                  {level === 'ALL' ? 'All Levels' : level}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex-1 relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            type="text"
            placeholder="Search logs..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9 h-8"
          />
        </div>

        <div className="flex items-center gap-2 text-sm">
          <span
            className={cn(
              'w-2 h-2 rounded-full',
              isConnected ? 'bg-green-500' : 'bg-destructive'
            )}
          />
          <span className="text-muted-foreground">
            {filteredLogs.length} logs
          </span>
        </div>
      </div>

      {/* Log list */}
      <div className="flex-1 min-h-0">
        {filteredLogs.length === 0 && !error ? (
          <div className="p-4 text-center text-muted-foreground text-sm">
            {logs.length === 0
              ? 'No logs available'
              : 'No logs match the current filters'}
          </div>
        ) : error ? (
          <div className="p-4 text-center text-destructive text-sm">
            <AlertCircle className="h-4 w-4 inline mr-2" />
            {error}
          </div>
        ) : (
          <Virtuoso<ServerLogEntry>
            ref={virtuosoRef}
            className="h-full"
            data={filteredLogs}
            itemContent={(_, entry) => <LogEntryRow entry={entry} />}
            atBottomStateChange={setAtBottom}
            followOutput={atBottom ? 'smooth' : false}
            increaseViewportBy={{ top: 0, bottom: 600 }}
          />
        )}
      </div>
    </div>
  );
}

export default ServerLogsViewer;
