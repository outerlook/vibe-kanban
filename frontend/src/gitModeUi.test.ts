import { describe, expect, it } from 'vitest';

import gitOperationsSource from './components/tasks/Toolbar/GitOperations.tsx?raw';
import branchCommitDiffMenuSource from './components/tasks/Toolbar/BranchCommitDiffMenu.tsx?raw';
import taskGroupFormDialogSource from './components/dialogs/tasks/TaskGroupFormDialog.tsx?raw';
import groupCardSource from './components/tasks/GroupCard.tsx?raw';
import {
  buildMergeTaskAttemptRequest,
  type MergeParams,
} from './hooks/useMerge';
import {
  buildQueueMergeRequest,
  type QueueMergeParams,
} from './hooks/useMergeQueue';

describe('git mode UI wiring', () => {
  it('submits the selected task group git mode', () => {
    expect(taskGroupFormDialogSource).toContain(
      "const [gitMode, setGitMode] = useState<GitMode>('managed')"
    );
    expect(taskGroupFormDialogSource).toContain('setGitMode(group.git_mode)');
    expect(taskGroupFormDialogSource).toContain('git_mode: gitMode');
    expect(taskGroupFormDialogSource).toContain(
      '<SelectItem value="preserve_history">'
    );
  });

  it('shows preserve-history task groups without showing managed groups', () => {
    expect(groupCardSource).toContain("group.git_mode === 'preserve_history'");
    expect(groupCardSource).toContain('groupCard.gitMode.preserveHistory');
  });

  it('keeps the merge strategy override non-persistent', () => {
    expect(gitOperationsSource).toContain('radioGroups');
    expect(gitOperationsSource).toContain('selectedMergeStrategy');
    expect(gitOperationsSource).toContain('mergeStrategyOverride');
    expect(gitOperationsSource).not.toContain(
      "localStorage.setItem('vk-merge-strategy"
    );
  });

  it('opens commit difference lists from the ahead and behind chips', () => {
    expect(gitOperationsSource).toContain('BranchCommitDiffMenu');
    expect(gitOperationsSource).toContain('selectedRepoStatus?.ahead_commits');
    expect(gitOperationsSource).toContain('selectedRepoStatus?.behind_commits');
    expect(branchCommitDiffMenuSource).toContain('DropdownMenuTrigger asChild');
    expect(branchCommitDiffMenuSource).toContain('git.commitDiff.aheadTitle');
    expect(branchCommitDiffMenuSource).toContain('git.commitDiff.behindTitle');
    expect(branchCommitDiffMenuSource).toContain('shortSha(commit.oid)');
  });

  it('builds direct merge requests with an explicit strategy only when selected', () => {
    const defaultParams: MergeParams = { repoId: 'repo-1' };
    expect(buildMergeTaskAttemptRequest(defaultParams)).toMatchObject({
      repo_id: 'repo-1',
      merge_strategy: null,
    });

    expect(
      buildMergeTaskAttemptRequest({
        repoId: 'repo-1',
        mergeStrategy: 'fast_forward_target',
      })
    ).toMatchObject({
      repo_id: 'repo-1',
      merge_strategy: 'fast_forward_target',
    });
  });

  it('builds queued merge requests with an explicit strategy only when selected', () => {
    const defaultParams: QueueMergeParams = { repoId: 'repo-1' };
    expect(buildQueueMergeRequest(defaultParams)).toMatchObject({
      repo_id: 'repo-1',
      merge_strategy: null,
    });

    expect(
      buildQueueMergeRequest({
        repoId: 'repo-1',
        mergeStrategy: 'fast_forward_target',
      })
    ).toMatchObject({
      repo_id: 'repo-1',
      merge_strategy: 'fast_forward_target',
    });
  });
});
