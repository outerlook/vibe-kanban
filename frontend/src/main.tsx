import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App.tsx';
import './styles/index.css';
import { ClickToComponent } from 'click-to-react-component';
import { VibeKanbanWebCompanion } from 'vibe-kanban-web-companion';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import * as Sentry from '@sentry/react';
import i18n from './i18n';
import posthog from 'posthog-js';
import { PostHogProvider } from 'posthog-js/react';
import { initApiBaseUrl } from './lib/api';
// Import modal type definitions
import './types/modals';

import {
  useLocation,
  useNavigationType,
  createRoutesFromChildren,
  matchRoutes,
} from 'react-router-dom';

const registerEarlyErrorLogging = () => {
  const handleError = (event: ErrorEvent) => {
    console.error('[Early Error]', event.error ?? event.message);
  };
  const handleRejection = (event: PromiseRejectionEvent) => {
    console.error('[Early Error] Unhandled rejection', event.reason);
  };

  window.addEventListener('error', handleError);
  window.addEventListener('unhandledrejection', handleRejection);

  return () => {
    window.removeEventListener('error', handleError);
    window.removeEventListener('unhandledrejection', handleRejection);
  };
};

let analyticsInitialized = false;
let cleanupEarlyErrorLogging: null | (() => void) = registerEarlyErrorLogging();

function AnalyticsInitializer() {
  React.useEffect(() => {
    if (analyticsInitialized) return;
    analyticsInitialized = true;

    let sentryReady = false;
    try {
      Sentry.init({
        dsn: 'https://1065a1d276a581316999a07d5dffee26@o4509603705192449.ingest.de.sentry.io/4509605576441937',
        tracesSampleRate: 1.0,
        environment:
          import.meta.env.MODE === 'development' ? 'dev' : 'production',
        integrations: [
          Sentry.reactRouterV6BrowserTracingIntegration({
            useEffect: React.useEffect,
            useLocation,
            useNavigationType,
            createRoutesFromChildren,
            matchRoutes,
          }),
        ],
      });
      Sentry.setTag('source', 'frontend');
      sentryReady = true;
    } catch (error) {
      console.error('[Analytics] Sentry init failed', error);
    }

    try {
      if (
        import.meta.env.VITE_POSTHOG_API_KEY &&
        import.meta.env.VITE_POSTHOG_API_ENDPOINT
      ) {
        posthog.init(import.meta.env.VITE_POSTHOG_API_KEY, {
          api_host: import.meta.env.VITE_POSTHOG_API_ENDPOINT,
          capture_pageview: false,
          capture_pageleave: true,
          capture_performance: true,
          autocapture: false,
          opt_out_capturing_by_default: true,
        });
      } else {
        console.warn(
          'PostHog API key or endpoint not set. Analytics will be disabled.'
        );
      }
    } catch (error) {
      console.error('[Analytics] PostHog init failed', error);
    }

    if (sentryReady && cleanupEarlyErrorLogging) {
      cleanupEarlyErrorLogging();
      cleanupEarlyErrorLogging = null;
    }
  }, []);

  return null;
}

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000 * 60 * 5, // 5 minutes
      refetchOnWindowFocus: false,
    },
  },
});

// Initialize API base URL before rendering to ensure getApiBaseUrlSync() works
// This is critical for Tauri where the server runs on a dynamic port
initApiBaseUrl().then(() => {
  ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AnalyticsInitializer />
    <QueryClientProvider client={queryClient}>
      <PostHogProvider client={posthog}>
        <Sentry.ErrorBoundary
          fallback={<p>{i18n.t('common:states.error')}</p>}
          showDialog
        >
          <ClickToComponent />
          <VibeKanbanWebCompanion />
          <App />
          {/*<TanStackDevtools plugins={[FormDevtoolsPlugin()]} />*/}
          {/* <ReactQueryDevtools initialIsOpen={false} /> */}
        </Sentry.ErrorBoundary>
      </PostHogProvider>
    </QueryClientProvider>
  </React.StrictMode>
  );
});
