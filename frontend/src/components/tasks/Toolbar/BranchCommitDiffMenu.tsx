import type { ReactNode } from 'react';
import { GitCommit } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { BranchStatusCommit } from 'shared/types';

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { formatRelativeTime } from '@/lib/utils';

type CommitDiffDirection = 'ahead' | 'behind';

type BranchCommitDiffMenuProps = {
  direction: CommitDiffDirection;
  count: number;
  targetBranchName: string;
  commits: BranchStatusCommit[];
  truncated: boolean;
  children: ReactNode;
};

function shortSha(oid: string) {
  return oid.slice(0, 7);
}

function formatCommitTime(authoredAt: string | null) {
  if (!authoredAt) return null;
  const date = new Date(authoredAt);
  if (Number.isNaN(date.getTime())) return null;
  return formatRelativeTime(authoredAt);
}

export function BranchCommitDiffMenu({
  direction,
  count,
  targetBranchName,
  commits,
  truncated,
  children,
}: BranchCommitDiffMenuProps) {
  const { t } = useTranslation('tasks');
  const commitLabel = t('git.status.commits', { count });
  const title =
    direction === 'ahead'
      ? t('git.commitDiff.aheadTitle', {
          count,
          commitLabel,
          branch: targetBranchName,
        })
      : t('git.commitDiff.behindTitle', {
          count,
          commitLabel,
          branch: targetBranchName,
        });

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        sideOffset={6}
        className="w-[min(420px,calc(100vw-2rem))] p-0"
      >
        <div className="border-b px-3 py-2">
          <div className="text-sm font-medium">{title}</div>
        </div>
        <div className="max-h-72 overflow-y-auto py-1">
          {commits.length === 0 ? (
            <div className="px-3 py-3 text-sm text-muted-foreground">
              {t('git.commitDiff.empty')}
            </div>
          ) : (
            commits.map((commit) => {
              const commitTime = formatCommitTime(commit.authored_at);

              return (
                <div
                  key={commit.oid}
                  className="grid grid-cols-[auto_1fr] gap-x-2 px-3 py-2 text-sm"
                  title={commit.oid}
                >
                  <GitCommit className="mt-0.5 h-3.5 w-3.5 text-muted-foreground" />
                  <div className="min-w-0">
                    <div className="flex min-w-0 items-baseline gap-2">
                      <span className="font-mono text-xs text-muted-foreground">
                        {shortSha(commit.oid)}
                      </span>
                      <span className="truncate">{commit.subject}</span>
                    </div>
                    {(commit.author_name || commitTime) && (
                      <div className="mt-0.5 truncate text-xs text-muted-foreground">
                        {[commit.author_name, commitTime]
                          .filter(Boolean)
                          .join(' - ')}
                      </div>
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
        {truncated && (
          <div className="border-t px-3 py-2 text-xs text-muted-foreground">
            {t('git.commitDiff.truncated', { count: commits.length })}
          </div>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
