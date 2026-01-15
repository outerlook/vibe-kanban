import { Loader2, Plus, FolderOpen } from 'lucide-react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { GroupCard } from './GroupCard';
import { useTaskGroupStats } from '@/hooks/useTaskGroupStats';
import { useProjectRepos } from '@/hooks/useProjectRepos';
import { TaskGroupFormDialog } from '@/components/dialogs/tasks/TaskGroupFormDialog';

interface GroupViewProps {
  projectId: string;
  onGroupClick?: (groupId: string) => void;
}

export function GroupView({ projectId, onGroupClick }: GroupViewProps) {
  const {
    data: groups,
    isLoading: isLoadingGroups,
    error: groupsError,
  } = useTaskGroupStats(projectId);

  const { data: repos, isLoading: isLoadingRepos } = useProjectRepos(projectId);

  const isLoading = isLoadingGroups || isLoadingRepos;
  const repoId = repos?.[0]?.id;

  const handleCreateGroup = async () => {
    try {
      await TaskGroupFormDialog.show({ mode: 'create', projectId });
    } catch {
      // User cancelled
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
        Loading groups...
      </div>
    );
  }

  if (groupsError) {
    return (
      <Alert variant="destructive">
        <AlertDescription>
          {groupsError.message || 'Failed to load groups'}
        </AlertDescription>
      </Alert>
    );
  }

  if (!groups || groups.length === 0) {
    return (
      <Card>
        <CardContent className="py-12 text-center">
          <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-lg bg-muted">
            <FolderOpen className="h-6 w-6" />
          </div>
          <h3 className="mt-4 text-lg font-semibold">No groups yet</h3>
          <p className="mt-2 text-sm text-muted-foreground">
            Create a group to organize your tasks by feature or sprint.
          </p>
          <Button className="mt-4" onClick={handleCreateGroup}>
            <Plus className="mr-2 h-4 w-4" />
            Create Group
          </Button>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
      {groups.map((group) => (
        <GroupCard
          key={group.id}
          group={group}
          repoId={repoId ?? ''}
          onClick={onGroupClick ? () => onGroupClick(group.id) : undefined}
        />
      ))}
    </div>
  );
}
