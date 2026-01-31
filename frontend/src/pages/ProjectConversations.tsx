import { useParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useProject } from '@/contexts/ProjectContext';
import { Loader } from '@/components/ui/loader';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { AlertTriangle } from 'lucide-react';
import { ConversationPanel } from '@/components/conversations/ConversationPanel';
import { useMediaQuery } from '@/hooks/useMediaQuery';

export function ProjectConversations() {
  const { t } = useTranslation(['common']);
  const { conversationId } = useParams<{
    projectId: string;
    conversationId?: string;
  }>();
  const { projectId, isLoading, error } = useProject();
  const isDesktop = useMediaQuery('(min-width: 1280px)');
  const isMobile = !isDesktop;

  if (error) {
    return (
      <div className="p-4">
        <Alert>
          <AlertTitle className="flex items-center gap-2">
            <AlertTriangle size="16" />
            {t('common:states.error')}
          </AlertTitle>
          <AlertDescription>
            {error.message || 'Failed to load project'}
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  if (isLoading || !projectId) {
    return <Loader message={t('common:states.loading')} size={32} className="py-8" />;
  }

  return (
    <div className="h-full p-4">
      <ConversationPanel projectId={projectId} initialConversationId={conversationId} isMobile={isMobile} />
    </div>
  );
}
