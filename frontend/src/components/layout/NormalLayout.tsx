import { Outlet, useSearchParams } from 'react-router-dom';
import { DevBanner } from '@/components/DevBanner';
import { Navbar } from '@/components/layout/Navbar';
import { BottomTabBar } from '@/components/layout/BottomTabBar';
import { useMediaQuery } from '@/hooks/useMediaQuery';
import { cn } from '@/lib/utils';

export function NormalLayout() {
  const [searchParams] = useSearchParams();
  const view = searchParams.get('view');
  const shouldHideNav = view === 'preview' || view === 'diffs';
  const isMobile = !useMediaQuery('(min-width: 1280px)');

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
