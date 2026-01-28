import { User, Check, Plus, Trash2, ChevronDown, AlertCircle } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useAccountInfo } from '@/hooks/useAccountInfo';
import {
  useClaudeAccounts,
  useCurrentClaudeAccount,
  useSwitchClaudeAccount,
  useDeleteClaudeAccount,
} from '@/hooks/useClaudeAccounts';
import { SaveAccountDialog } from '@/components/dialogs/account/SaveAccountDialog';
import { ConfirmDialog } from '@/components/dialogs/shared/ConfirmDialog';
import { UsageLimitDisplay } from '@/components/layout/UsageLimitDisplay';
import { cn } from '@/lib/utils';

export function ClaudeAccountSwitcher() {
  const { data: accountInfo, isLoading: isAccountInfoLoading } =
    useAccountInfo();
  const { data: accounts = [], isLoading: isAccountsLoading } =
    useClaudeAccounts();
  const { data: currentHashPrefix, isLoading: isCurrentLoading } =
    useCurrentClaudeAccount();

  const switchAccount = useSwitchClaudeAccount();
  const deleteAccount = useDeleteClaudeAccount();

  const isLoading = isAccountInfoLoading || isAccountsLoading || isCurrentLoading;
  const claudeInfo = accountInfo?.claude;

  // Check if current account is saved
  const isCurrentAccountSaved =
    currentHashPrefix && accounts.some((a) => a.hashPrefix === currentHashPrefix);

  const getAccountDisplayName = (account: {
    name: string | null;
    hashPrefix: string;
  }) => account.name ?? account.hashPrefix;

  const getTriggerLabel = () => {
    if (isLoading) return 'Loading...';
    if (claudeInfo?.subscriptionType) {
      return claudeInfo.subscriptionType;
    }
    return 'Account';
  };

  const handleSwitchAccount = async (hashPrefix: string) => {
    const account = accounts.find((a) => a.hashPrefix === hashPrefix);
    if (!account) return;

    const displayName = getAccountDisplayName(account);
    const result = await ConfirmDialog.show({
      title: 'Switch Account?',
      message: `Switch to account "${displayName}"? This will update your Claude credentials.`,
      confirmText: 'Switch',
      cancelText: 'Cancel',
      variant: 'info',
    });

    if (result === 'confirmed') {
      switchAccount.mutate(hashPrefix);
    }
  };

  const handleDeleteAccount = async (
    hashPrefix: string,
    e: React.MouseEvent
  ) => {
    e.stopPropagation();

    const account = accounts.find((a) => a.hashPrefix === hashPrefix);
    if (!account) return;

    const displayName = getAccountDisplayName(account);
    const result = await ConfirmDialog.show({
      title: 'Delete Account?',
      message: `Delete saved account "${displayName}"? This will only remove the saved account entry, not the actual Claude account.`,
      confirmText: 'Delete',
      cancelText: 'Keep',
      variant: 'destructive',
    });

    if (result === 'confirmed') {
      deleteAccount.mutate(hashPrefix);
    }
  };

  const handleSaveCurrentAccount = async () => {
    await SaveAccountDialog.show({
      currentSubscriptionType: claudeInfo?.subscriptionType,
    });
  };

  return (
    <DropdownMenu>
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
                  <UsageLimitDisplay
                    label="Session (5h)"
                    usedPercent={claudeInfo.usage.fiveHour.usedPercent}
                    resetsAt={claudeInfo.usage.fiveHour.resetsAt}
                  />
                  <UsageLimitDisplay
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
        {!isLoading && currentHashPrefix && !isCurrentAccountSaved && (
          <>
            <DropdownMenuItem
              onClick={handleSaveCurrentAccount}
              className="text-amber-600 dark:text-amber-400"
            >
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
                  onClick={() =>
                    !isCurrent && handleSwitchAccount(account.hashPrefix)
                  }
                  className={cn(
                    'justify-between group',
                    isCurrent && 'bg-accent'
                  )}
                  disabled={isCurrent || switchAccount.isPending}
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
                        onClick={(e) => handleDeleteAccount(account.hashPrefix, e)}
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
        <DropdownMenuItem onClick={handleSaveCurrentAccount}>
          <Plus className="mr-2 h-4 w-4" />
          Save current account
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
