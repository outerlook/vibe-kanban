import { Suspense, lazy, useEffect } from 'react';
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/i18n';
import { NormalLayout } from '@/components/layout/NormalLayout';
import { usePostHog } from 'posthog-js/react';
import { useAuth } from '@/hooks';
import { usePreviousPath } from '@/hooks/usePreviousPath';

import { UserSystemProvider, useUserSystem } from '@/components/ConfigProvider';
import { ThemeProvider } from '@/components/ThemeProvider';
import { SearchProvider } from '@/contexts/SearchContext';

import { HotkeysProvider } from 'react-hotkeys-hook';

import { ProjectProvider } from '@/contexts/ProjectContext';
import { UnreadProvider } from '@/contexts/UnreadContext';
import { TaskSelectionProvider } from '@/contexts/TaskSelectionContext';
import { BulkActionsBar } from '@/components/tasks/BulkActionsBar';
import { ThemeMode } from 'shared/types';
import * as Sentry from '@sentry/react';
import { Loader } from '@/components/ui/loader';

import { DisclaimerDialog } from '@/components/dialogs/global/DisclaimerDialog';
import { OnboardingDialog } from '@/components/dialogs/global/OnboardingDialog';
import { ReleaseNotesDialog } from '@/components/dialogs/global/ReleaseNotesDialog';
import { CommandPalette } from '@/components/CommandPalette';
import { ClickedElementsProvider } from './contexts/ClickedElementsProvider';
import NiceModal from '@ebay/nice-modal-react';

const Projects = lazy(() =>
  import('@/pages/Projects').then((module) => ({
    default: module.Projects,
  }))
);
const ProjectTasks = lazy(() =>
  import('@/pages/ProjectTasks').then((module) => ({
    default: module.ProjectTasks,
  }))
);
const FullAttemptLogsPage = lazy(() =>
  import('@/pages/FullAttemptLogs').then((module) => ({
    default: module.FullAttemptLogsPage,
  }))
);
const SettingsLayout = lazy(() =>
  import('@/pages/settings/SettingsLayout').then((module) => ({
    default: module.SettingsLayout,
  }))
);
const GeneralSettings = lazy(() =>
  import('@/pages/settings/GeneralSettings').then((module) => ({
    default: module.GeneralSettings,
  }))
);
const ProjectSettings = lazy(() =>
  import('@/pages/settings/ProjectSettings').then((module) => ({
    default: module.ProjectSettings,
  }))
);
const OrganizationSettings = lazy(() =>
  import('@/pages/settings/OrganizationSettings').then((module) => ({
    default: module.OrganizationSettings,
  }))
);
const AgentSettings = lazy(() =>
  import('@/pages/settings/AgentSettings').then((module) => ({
    default: module.AgentSettings,
  }))
);
const McpSettings = lazy(() =>
  import('@/pages/settings/McpSettings').then((module) => ({
    default: module.McpSettings,
  }))
);
const GanttView = lazy(() =>
  import('@/pages/GanttView').then((module) => ({
    default: module.GanttView,
  }))
);

const SentryRoutes = Sentry.withSentryReactRouterV6Routing(Routes);

function AppContent() {
  const { config, analyticsUserId, updateAndSaveConfig, loading } =
    useUserSystem();
  const posthog = usePostHog();
  const { isSignedIn } = useAuth();

  // Track previous path for back navigation
  usePreviousPath();

  // Handle opt-in/opt-out and user identification when config loads
  useEffect(() => {
    if (!posthog || !analyticsUserId) return;

    if (config?.analytics_enabled) {
      posthog.opt_in_capturing();
      posthog.identify(analyticsUserId);
      console.log('[Analytics] Analytics enabled and user identified');
    } else {
      posthog.opt_out_capturing();
      console.log('[Analytics] Analytics disabled by user preference');
    }
  }, [config?.analytics_enabled, analyticsUserId, posthog]);

  useEffect(() => {
    if (!config) return;
    let cancelled = false;

    const showNextStep = async () => {
      // 1) Disclaimer - first step
      if (!config.disclaimer_acknowledged) {
        await DisclaimerDialog.show();
        if (!cancelled) {
          await updateAndSaveConfig({ disclaimer_acknowledged: true });
        }
        DisclaimerDialog.hide();
        return;
      }

      // 2) Onboarding - configure executor and editor
      if (!config.onboarding_acknowledged) {
        const result = await OnboardingDialog.show();
        if (!cancelled) {
          await updateAndSaveConfig({
            onboarding_acknowledged: true,
            executor_profile: result.profile,
            editor: result.editor,
          });
        }
        OnboardingDialog.hide();
        return;
      }

      // 3) Release notes - last step
      if (config.show_release_notes) {
        await ReleaseNotesDialog.show();
        if (!cancelled) {
          await updateAndSaveConfig({ show_release_notes: false });
        }
        ReleaseNotesDialog.hide();
        return;
      }
    };

    showNextStep();

    return () => {
      cancelled = true;
    };
  }, [config, isSignedIn, updateAndSaveConfig]);

  if (loading) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <Loader message="Loading..." size={32} />
      </div>
    );
  }

  return (
    <I18nextProvider i18n={i18n}>
      <ThemeProvider initialTheme={config?.theme || ThemeMode.SYSTEM}>
        <SearchProvider>
          <CommandPalette />
          <div className="h-screen flex flex-col bg-background">
            <Suspense
              fallback={
                <div className="min-h-screen bg-background flex items-center justify-center">
                  <Loader message="Loading..." size={32} />
                </div>
              }
            >
              <SentryRoutes>
                {/* VS Code full-page logs route (outside NormalLayout for minimal UI) */}
                <Route
                  path="/projects/:projectId/tasks/:taskId/attempts/:attemptId/full"
                  element={<FullAttemptLogsPage />}
                />

                <Route element={<NormalLayout />}>
                  <Route path="/" element={<Projects />} />
                  <Route path="/projects" element={<Projects />} />
                  <Route path="/projects/:projectId" element={<Projects />} />
                  <Route
                    path="/projects/:projectId/tasks"
                    element={<ProjectTasks />}
                  />
                  <Route
                    path="/projects/:projectId/gantt"
                    element={<GanttView />}
                  />
                  <Route
                    path="/projects/:projectId/gantt/:taskId"
                    element={<GanttView />}
                  />
                  <Route
                    path="/projects/:projectId/gantt/:taskId/attempts/:attemptId"
                    element={<GanttView />}
                  />
                  <Route path="/settings/*" element={<SettingsLayout />}>
                    <Route index element={<Navigate to="general" replace />} />
                    <Route path="general" element={<GeneralSettings />} />
                    <Route path="projects" element={<ProjectSettings />} />
                    <Route
                      path="organizations"
                      element={<OrganizationSettings />}
                    />
                    <Route path="agents" element={<AgentSettings />} />
                    <Route path="mcp" element={<McpSettings />} />
                  </Route>
                  <Route
                    path="/mcp-servers"
                    element={<Navigate to="/settings/mcp" replace />}
                  />
                  <Route
                    path="/projects/:projectId/tasks/:taskId"
                    element={<ProjectTasks />}
                  />
                  <Route
                    path="/projects/:projectId/tasks/:taskId/attempts/:attemptId"
                    element={<ProjectTasks />}
                  />
                </Route>
              </SentryRoutes>
            </Suspense>
          </div>
        </SearchProvider>
      </ThemeProvider>
    </I18nextProvider>
  );
}

function App() {
  return (
    <BrowserRouter>
      <UserSystemProvider>
        <UnreadProvider>
          <ClickedElementsProvider>
            <ProjectProvider>
              <TaskSelectionProvider>
                <HotkeysProvider
                  initiallyActiveScopes={['*', 'global', 'kanban']}
                >
                  <NiceModal.Provider>
                    <AppContent />
                    <BulkActionsBar />
                  </NiceModal.Provider>
                </HotkeysProvider>
              </TaskSelectionProvider>
            </ProjectProvider>
          </ClickedElementsProvider>
        </UnreadProvider>
      </UserSystemProvider>
    </BrowserRouter>
  );
}

export default App;
