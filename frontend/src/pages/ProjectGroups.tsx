import { useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';

import { useProject } from '@/contexts/ProjectContext';
import { GroupView } from '@/components/tasks/GroupView';
import { Loader } from '@/components/ui/loader';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { paths } from '@/lib/paths';

export function ProjectGroups() {
  const { t } = useTranslation(['tasks', 'common']);
  const navigate = useNavigate();

  const {
    projectId,
    isLoading: projectLoading,
    error: projectError,
  } = useProject();

  const handleGroupClick = useCallback(
    (groupId: string) => {
      if (!projectId) return;
      navigate(`${paths.projectTasks(projectId)}?group=${groupId}`);
    },
    [projectId, navigate]
  );

  if (projectError) {
    return (
      <div className="p-4">
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('common:states.error')}</AlertTitle>
          <AlertDescription>
            {projectError.message || 'Failed to load project'}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  if (projectLoading || !projectId) {
    return <Loader message={t('loading')} size={32} className="py-8" />;
  }

  return (
    <div className="h-full flex flex-col p-4">
      <GroupView projectId={projectId} onGroupClick={handleGroupClick} />
    </div>
  );
}
