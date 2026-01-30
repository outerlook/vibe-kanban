import { Outlet, useParams, useSearchParams } from 'react-router-dom';
import { DevBanner } from '@/components/DevBanner';
import { Navbar } from '@/components/layout/Navbar';
import { BottomTabBar } from '@/components/layout/BottomTabBar';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import { cn } from '@/lib/utils';

export function NormalLayout() {
  const [searchParams] = useSearchParams();
  const { taskId } = useParams<{ taskId?: string }>();
  const view = searchParams.get('view');
  const isMobile = !useMediaQuery('(min-width: 1280px)');
  // Only hide nav on mobile when viewing diffs/preview for a specific task
  const shouldHideNav =
    isMobile && !!taskId && (view === 'preview' || view === 'diffs');

  return (
    <>
      <DevBanner />
      {!shouldHideNav && <Navbar />}
      <div
        className={cn(
          'flex-1 min-h-0 overflow-hidden',
          isMobile && !shouldHideNav && 'pb-16'
        )}
      >
        <Outlet />
      </div>
      {isMobile && !shouldHideNav && <BottomTabBar />}
    </>
  );
}
