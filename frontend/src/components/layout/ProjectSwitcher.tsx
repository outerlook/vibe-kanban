import { useNavigate } from 'react-router-dom';
import { Check, ChevronDown, Circle, FolderOpen } from 'lucide-react';
import { useProjects } from '@/hooks/useProjects';
import { useUnread } from '@/contexts/UnreadContext';
import { useProject } from '@/contexts/ProjectContext';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';

export function ProjectSwitcher() {
  const navigate = useNavigate();
  const { project: currentProject, projectId } = useProject();
  const { projects, isLoading } = useProjects();
  const { getProjectUnreadCount } = useUnread();

  if (!projectId) return null;

  const triggerLabel =
    currentProject?.name ?? (isLoading ? 'Loading...' : 'Project');

  return (
    <DropdownMenu>
      <DropdownMenuTrigger className="flex items-center gap-1 rounded px-2 py-1 text-sm font-medium hover:bg-muted">
        <FolderOpen className="h-4 w-4" />
        <span className="max-w-[150px] truncate">{triggerLabel}</span>
        <ChevronDown className="h-3 w-3 opacity-50" />
      </DropdownMenuTrigger>

      <DropdownMenuContent align="start" className="w-[220px]">
        {isLoading ? (
          <DropdownMenuItem disabled>Loading projects...</DropdownMenuItem>
        ) : projects.length ? (
          projects.map((project) => {
            const isCurrent = project.id === projectId;
            const unreadCount = getProjectUnreadCount(project.id);
            const hasUnread = unreadCount !== undefined && unreadCount > 0;
            return (
              <DropdownMenuItem
                key={project.id}
                onClick={() => navigate(`/projects/${project.id}/tasks`)}
                className={cn('justify-between', isCurrent && 'bg-accent')}
                aria-current={isCurrent ? 'page' : undefined}
              >
                <span className="flex items-center gap-1.5 truncate">
                  {project.name}
                  {hasUnread && (
                    <Circle className="h-2.5 w-2.5 fill-amber-500 text-amber-500 shrink-0" />
                  )}
                </span>
                {isCurrent ? (
                  <Check className="h-4 w-4 text-primary" />
                ) : null}
              </DropdownMenuItem>
            );
          })
        ) : (
          <DropdownMenuItem disabled>No projects found</DropdownMenuItem>
        )}

        <DropdownMenuSeparator />

        <DropdownMenuItem onClick={() => navigate('/projects')}>
          <FolderOpen className="h-4 w-4" />
          All Projects
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
