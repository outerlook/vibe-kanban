import { useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { useCallback, useMemo } from 'react';
import {
  Kanban,
  MessageCircle,
  BarChart3,
  GitPullRequest,
  FileText,
  Eye,
  Code,
  type LucideIcon,
} from 'lucide-react';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import { useProject } from '@/contexts/ProjectContext';
import { paths } from '@/lib/paths';
import { cn } from '@/lib/utils';

interface TabConfig {
  id: string;
  label: string;
  icon: LucideIcon;
  isActive: boolean;
  onClick: () => void;
}

export function BottomTabBar() {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { taskId } = useParams<{ taskId?: string }>();
  const { projectId } = useProject();
  const isDesktop = useMediaQuery('(min-width: 1280px)');

  const isTasksRoute = /^\/projects\/[^/]+\/tasks/.test(location.pathname);
  const isConversationsRoute = /^\/projects\/[^/]+\/conversations/.test(location.pathname);
  const isGanttRoute = /^\/projects\/[^/]+\/gantt/.test(location.pathname);
  const isPrsRoute = /^\/projects\/[^/]+\/prs/.test(location.pathname);

  const hasTaskSelected = isTasksRoute && !!taskId;
  const currentView = searchParams.get('view') as 'preview' | 'diffs' | null;

  const navigatePreservingParams = useCallback(
    (path: string) => {
      const params = new URLSearchParams(searchParams);
      // These are task-page specific, don't carry to other routes
      params.delete('view');
      params.delete('view_mode');
      const search = params.toString();
      navigate({ pathname: path, search: search ? `?${search}` : '' });
    },
    [navigate, searchParams]
  );

  const setViewMode = useCallback(
    (mode: 'preview' | 'diffs' | null) => {
      const params = new URLSearchParams(searchParams);
      if (mode === null) {
        params.delete('view');
      } else {
        params.set('view', mode);
      }
      setSearchParams(params, { replace: true });
    },
    [searchParams, setSearchParams]
  );

  const tabs: TabConfig[] = useMemo(() => {
    if (!projectId) return [];

    if (hasTaskSelected) {
      return [
        {
          id: 'board',
          label: 'Board',
          icon: Kanban,
          isActive: false,
          onClick: () => navigate(paths.projectTasks(projectId)),
        },
        {
          id: 'details',
          label: 'Details',
          icon: FileText,
          isActive: currentView === null,
          onClick: () => setViewMode(null),
        },
        {
          id: 'preview',
          label: 'Preview',
          icon: Eye,
          isActive: currentView === 'preview',
          onClick: () => setViewMode('preview'),
        },
        {
          id: 'diffs',
          label: 'Diffs',
          icon: Code,
          isActive: currentView === 'diffs',
          onClick: () => setViewMode('diffs'),
        },
      ];
    }

    return [
      {
        id: 'board',
        label: 'Board',
        icon: Kanban,
        isActive: isTasksRoute,
        onClick: () => navigatePreservingParams(paths.projectTasks(projectId)),
      },
      {
        id: 'chat',
        label: 'Chat',
        icon: MessageCircle,
        isActive: isConversationsRoute,
        onClick: () => navigatePreservingParams(paths.projectConversations(projectId)),
      },
      {
        id: 'gantt',
        label: 'Gantt',
        icon: BarChart3,
        isActive: isGanttRoute,
        onClick: () => navigatePreservingParams(paths.projectGantt(projectId)),
      },
      {
        id: 'prs',
        label: 'PRs',
        icon: GitPullRequest,
        isActive: isPrsRoute,
        onClick: () => navigatePreservingParams(paths.projectPrs(projectId)),
      },
    ];
  }, [
    hasTaskSelected,
    projectId,
    currentView,
    isTasksRoute,
    isConversationsRoute,
    isGanttRoute,
    isPrsRoute,
    navigate,
    navigatePreservingParams,
    setViewMode,
  ]);

  // Don't render on desktop or without a project
  if (isDesktop || !projectId) {
    return null;
  }

  return (
    <nav
      className="fixed bottom-0 left-0 right-0 z-30 border-t bg-background xl:hidden pb-[max(0.5rem,env(safe-area-inset-bottom))]"
      aria-label="Mobile navigation"
    >
      <div className="flex items-center justify-around h-14">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.id}
              type="button"
              onClick={tab.onClick}
              className={cn(
                'flex flex-col items-center justify-center flex-1 h-full gap-1 transition-colors',
                'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
                tab.isActive ? 'text-primary' : 'text-muted-foreground'
              )}
              aria-current={tab.isActive ? 'page' : undefined}
            >
              <Icon className="h-5 w-5" />
              <span className="text-xs font-medium">{tab.label}</span>
            </button>
          );
        })}
      </div>
    </nav>
  );
}
