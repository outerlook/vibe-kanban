import { ServerLogsViewer } from '@/components/settings/ServerLogsViewer';

export function ServerLogs() {
  return (
    <div className="flex flex-col h-full">
      <div className="mb-4">
        <h2 className="text-lg font-semibold">Server Logs</h2>
        <p className="text-sm text-muted-foreground">
          Live stream of server-side tracing logs
        </p>
      </div>
      <div className="flex-1 min-h-0 border rounded-lg overflow-hidden">
        <ServerLogsViewer />
      </div>
    </div>
  );
}
