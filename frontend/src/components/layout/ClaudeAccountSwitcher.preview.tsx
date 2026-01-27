/**
 * Preview component for ClaudeAccountSwitcher
 *
 * This file demonstrates the dropdown states with mock data for visual verification.
 * Use ui-preview to verify appearance in different states.
 *
 * Usage with ui-preview:
 *   1. Import this component into a page/route
 *   2. Run the dev server
 *   3. Use ui-preview skill to take screenshots
 */

import { useState } from 'react';
import {
  User,
  Check,
  Plus,
  Trash2,
  ChevronDown,
  AlertCircle,
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';

// Mock types matching SavedAccount
interface MockAccount {
  hashPrefix: string;
  name: string | null;
  subscriptionType: string;
  rateLimitTier: string | null;
  createdAt: string;
}

interface MockClaudeInfo {
  subscriptionType: string;
  rateLimitTier: string | null;
  usage: {
    fiveHour: { usedPercent: number; resetsAt: string };
    sevenDay: { usedPercent: number; resetsAt: string };
  } | null;
}

// Mock data for different states
const mockAccounts: MockAccount[] = [
  {
    hashPrefix: 'a1b2c3d4',
    name: 'Work Account',
    subscriptionType: 'pro',
    rateLimitTier: 'tier_4',
    createdAt: '2024-01-15T10:00:00Z',
  },
  {
    hashPrefix: 'e5f6g7h8',
    name: 'Personal',
    subscriptionType: 'free',
    rateLimitTier: null,
    createdAt: '2024-02-20T14:30:00Z',
  },
  {
    hashPrefix: 'i9j0k1l2',
    name: null,
    subscriptionType: 'team',
    rateLimitTier: 'tier_5',
    createdAt: '2024-03-10T09:15:00Z',
  },
];

const mockClaudeInfo: MockClaudeInfo = {
  subscriptionType: 'pro',
  rateLimitTier: 'tier_4',
  usage: {
    fiveHour: {
      usedPercent: 35,
      resetsAt: new Date(Date.now() + 3 * 60 * 60 * 1000).toISOString(),
    },
    sevenDay: {
      usedPercent: 68,
      resetsAt: new Date(Date.now() + 4 * 24 * 60 * 60 * 1000).toISOString(),
    },
  },
};

type PreviewState =
  | 'no-accounts'
  | 'one-account'
  | 'multiple-accounts'
  | 'loading'
  | 'unsaved-current';

function MockUsageLimitDisplay({
  label,
  usedPercent,
}: {
  label: string;
  usedPercent: number;
  resetsAt: string;
}) {
  const getColor = () => {
    if (usedPercent >= 90) return 'bg-destructive';
    if (usedPercent >= 70) return 'bg-amber-500';
    return 'bg-primary';
  };

  return (
    <div className="text-xs">
      <div className="flex justify-between mb-1">
        <span className="text-muted-foreground">{label}</span>
        <span>{usedPercent}%</span>
      </div>
      <div className="h-1.5 bg-muted rounded-full overflow-hidden">
        <div
          className={cn('h-full rounded-full transition-all', getColor())}
          style={{ width: `${usedPercent}%` }}
        />
      </div>
    </div>
  );
}

function MockDropdown({
  state,
  currentHashPrefix,
}: {
  state: PreviewState;
  currentHashPrefix: string | null;
}) {
  const accounts =
    state === 'no-accounts'
      ? []
      : state === 'one-account'
        ? [mockAccounts[0]]
        : mockAccounts;

  const claudeInfo = state === 'loading' ? null : mockClaudeInfo;
  const isLoading = state === 'loading';
  const isCurrentAccountSaved =
    currentHashPrefix && accounts.some((a) => a.hashPrefix === currentHashPrefix);

  const getAccountDisplayName = (account: MockAccount) => {
    return account.name || account.hashPrefix;
  };

  const getTriggerLabel = () => {
    if (isLoading) return 'Loading...';
    if (claudeInfo?.subscriptionType) {
      return claudeInfo.subscriptionType;
    }
    return 'Account';
  };

  return (
    <DropdownMenu defaultOpen>
      <DropdownMenuTrigger className="flex items-center gap-1 rounded px-2 py-1 text-sm font-medium hover:bg-muted h-9">
        <User className="h-4 w-4" />
        <span className="max-w-[100px] truncate">{getTriggerLabel()}</span>
        <ChevronDown className="h-3 w-3 opacity-50" />
      </DropdownMenuTrigger>

      <DropdownMenuContent align="end" className="w-[280px]">
        {/* Current account info section */}
        {claudeInfo && (
          <>
            <div className="px-2 py-2 text-sm">
              <div className="font-medium">Current Account</div>
              <div className="text-muted-foreground">
                {claudeInfo.subscriptionType}
                {claudeInfo.rateLimitTier && (
                  <span className="text-xs ml-1">
                    ({claudeInfo.rateLimitTier})
                  </span>
                )}
              </div>
              {claudeInfo.usage && (
                <div className="mt-2 space-y-1">
                  <MockUsageLimitDisplay
                    label="Session (5h)"
                    usedPercent={claudeInfo.usage.fiveHour.usedPercent}
                    resetsAt={claudeInfo.usage.fiveHour.resetsAt}
                  />
                  <MockUsageLimitDisplay
                    label="Weekly"
                    usedPercent={claudeInfo.usage.sevenDay.usedPercent}
                    resetsAt={claudeInfo.usage.sevenDay.resetsAt}
                  />
                </div>
              )}
            </div>
            <DropdownMenuSeparator />
          </>
        )}

        {/* Unsaved account prompt */}
        {!isLoading &&
          currentHashPrefix &&
          !isCurrentAccountSaved &&
          state === 'unsaved-current' && (
            <>
              <DropdownMenuItem className="text-amber-600 dark:text-amber-400">
                <AlertCircle className="mr-2 h-4 w-4" />
                Current account not saved
              </DropdownMenuItem>
              <DropdownMenuSeparator />
            </>
          )}

        {/* Saved accounts list */}
        <div className="max-h-[200px] overflow-y-auto">
          {isLoading ? (
            <DropdownMenuItem disabled>Loading accounts...</DropdownMenuItem>
          ) : accounts.length === 0 ? (
            <DropdownMenuItem disabled className="text-muted-foreground">
              No saved accounts
            </DropdownMenuItem>
          ) : (
            accounts.map((account) => {
              const isCurrent = account.hashPrefix === currentHashPrefix;
              const displayName = getAccountDisplayName(account);

              return (
                <DropdownMenuItem
                  key={account.hashPrefix}
                  className={cn(
                    'justify-between group',
                    isCurrent && 'bg-accent'
                  )}
                  disabled={isCurrent}
                >
                  <span className="flex items-center gap-2 truncate">
                    <User className="h-4 w-4 shrink-0" />
                    <span className="truncate">{displayName}</span>
                    <span className="text-xs text-muted-foreground shrink-0">
                      {account.subscriptionType}
                    </span>
                  </span>
                  <span className="flex items-center gap-1">
                    {isCurrent && <Check className="h-4 w-4 text-primary" />}
                    {!isCurrent && (
                      <button
                        type="button"
                        className="opacity-0 group-hover:opacity-100 p-1 hover:bg-destructive/10 rounded transition-opacity"
                        aria-label={`Delete account ${displayName}`}
                      >
                        <Trash2 className="h-3 w-3 text-destructive" />
                      </button>
                    )}
                  </span>
                </DropdownMenuItem>
              );
            })
          )}
        </div>

        <DropdownMenuSeparator />

        {/* Actions */}
        <DropdownMenuItem>
          <Plus className="mr-2 h-4 w-4" />
          Save current account
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function ClaudeAccountSwitcherPreview() {
  const [selectedState, setSelectedState] = useState<PreviewState>('multiple-accounts');

  const states: { value: PreviewState; label: string }[] = [
    { value: 'no-accounts', label: 'No Accounts' },
    { value: 'one-account', label: 'One Account' },
    { value: 'multiple-accounts', label: 'Multiple Accounts' },
    { value: 'loading', label: 'Loading' },
    { value: 'unsaved-current', label: 'Unsaved Current' },
  ];

  const getCurrentHash = (state: PreviewState) => {
    if (state === 'no-accounts') return null;
    if (state === 'unsaved-current') return 'newaccount';
    return mockAccounts[0].hashPrefix;
  };

  return (
    <div className="p-8 space-y-8">
      <div>
        <h1 className="text-2xl font-bold mb-2">ClaudeAccountSwitcher Preview</h1>
        <p className="text-muted-foreground mb-4">
          Visual preview of the Claude account switcher dropdown in different states.
        </p>
      </div>

      <div className="space-y-4">
        <div className="flex flex-wrap gap-2">
          {states.map((s) => (
            <button
              key={s.value}
              onClick={() => setSelectedState(s.value)}
              className={`px-3 py-1 rounded text-sm ${
                selectedState === s.value
                  ? 'bg-primary text-primary-foreground'
                  : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
              }`}
            >
              {s.label}
            </button>
          ))}
        </div>
      </div>

      <div className="space-y-2">
        <h2 className="text-lg font-semibold">Dropdown Preview</h2>
        <p className="text-sm text-muted-foreground mb-4">
          State: <code className="bg-muted px-1 rounded">{selectedState}</code>
        </p>
        <div className="flex justify-end">
          <MockDropdown
            state={selectedState}
            currentHashPrefix={getCurrentHash(selectedState)}
          />
        </div>
      </div>

      <div className="space-y-4 mt-16">
        <h2 className="text-lg font-semibold">All States Side by Side</h2>
        <div className="grid grid-cols-2 gap-8">
          <div>
            <h3 className="text-sm font-medium mb-2">No Accounts</h3>
            <div className="flex justify-end">
              <MockDropdown state="no-accounts" currentHashPrefix={null} />
            </div>
          </div>
          <div>
            <h3 className="text-sm font-medium mb-2">One Account (current)</h3>
            <div className="flex justify-end">
              <MockDropdown
                state="one-account"
                currentHashPrefix={mockAccounts[0].hashPrefix}
              />
            </div>
          </div>
          <div>
            <h3 className="text-sm font-medium mb-2">Multiple Accounts</h3>
            <div className="flex justify-end">
              <MockDropdown
                state="multiple-accounts"
                currentHashPrefix={mockAccounts[0].hashPrefix}
              />
            </div>
          </div>
          <div>
            <h3 className="text-sm font-medium mb-2">Unsaved Current Account</h3>
            <div className="flex justify-end">
              <MockDropdown state="unsaved-current" currentHashPrefix="newaccount" />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export default ClaudeAccountSwitcherPreview;
