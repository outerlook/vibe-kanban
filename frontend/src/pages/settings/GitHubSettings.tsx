import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Loader2, Github, Download, Trash2, Eye, EyeOff } from 'lucide-react';
import {
  useGitHubSettings,
  useSetGitHubToken,
  useDeleteGitHubToken,
  useImportGitHubToken,
} from '@/hooks/useGitHubSettings';

export function GitHubSettings() {
  const { t } = useTranslation('settings');
  const [tokenInput, setTokenInput] = useState('');
  const [showToken, setShowToken] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const { data: settings, isLoading } = useGitHubSettings();
  const setToken = useSetGitHubToken();
  const deleteToken = useDeleteGitHubToken();
  const importToken = useImportGitHubToken();

  const isConfigured = settings?.configured ?? false;

  const handleSaveToken = async () => {
    if (!tokenInput.trim()) {
      setError(t('settings.github.token.errors.required'));
      return;
    }

    setError(null);
    try {
      await setToken.mutateAsync(tokenInput.trim());
      setTokenInput('');
      setSuccess(t('settings.github.token.success.saved'));
      setTimeout(() => setSuccess(null), 3000);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('settings.github.token.errors.saveFailed'));
    }
  };

  const handleDeleteToken = async () => {
    const confirmed = window.confirm(t('settings.github.token.confirmDelete'));
    if (!confirmed) return;

    setError(null);
    try {
      await deleteToken.mutateAsync();
      setSuccess(t('settings.github.token.success.deleted'));
      setTimeout(() => setSuccess(null), 3000);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('settings.github.token.errors.deleteFailed'));
    }
  };

  const handleImportFromCli = async () => {
    setError(null);
    try {
      const result = await importToken.mutateAsync();
      setSuccess(result.message);
      setTimeout(() => setSuccess(null), 3000);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('settings.github.token.errors.importFailed'));
    }
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="h-8 w-8 animate-spin" />
        <span className="ml-2">{t('settings.github.loading')}</span>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {success && (
        <Alert variant="success">
          <AlertDescription className="font-medium">{success}</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Github className="h-6 w-6" />
              <div>
                <CardTitle>{t('settings.github.token.title')}</CardTitle>
                <CardDescription>
                  {t('settings.github.token.description')}
                </CardDescription>
              </div>
            </div>
            <Badge
              variant={isConfigured ? 'default' : 'secondary'}
              className={isConfigured ? 'bg-green-600 hover:bg-green-600' : ''}
            >
              {isConfigured
                ? t('settings.github.token.status.configured')
                : t('settings.github.token.status.notConfigured')}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-6">
          {isConfigured ? (
            <div className="space-y-4">
              <div className="flex items-center gap-2 p-3 bg-muted rounded-md">
                <div className="flex-1">
                  <p className="text-sm text-muted-foreground">
                    {t('settings.github.token.tokenConfiguredMessage')}
                  </p>
                </div>
              </div>
              <div className="flex gap-2">
                <Button
                  variant="destructive"
                  onClick={handleDeleteToken}
                  disabled={deleteToken.isPending}
                >
                  {deleteToken.isPending ? (
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  ) : (
                    <Trash2 className="h-4 w-4 mr-2" />
                  )}
                  {t('settings.github.token.buttons.remove')}
                </Button>
              </div>
            </div>
          ) : (
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="github-token">
                  {t('settings.github.token.inputLabel')}
                </Label>
                <div className="flex gap-2">
                  <div className="relative flex-1">
                    <Input
                      id="github-token"
                      type={showToken ? 'text' : 'password'}
                      value={tokenInput}
                      onChange={(e) => setTokenInput(e.target.value)}
                      placeholder={t('settings.github.token.inputPlaceholder')}
                      className="pr-10"
                    />
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="absolute right-0 top-0 h-full px-3 hover:bg-transparent"
                      onClick={() => setShowToken(!showToken)}
                    >
                      {showToken ? (
                        <EyeOff className="h-4 w-4 text-muted-foreground" />
                      ) : (
                        <Eye className="h-4 w-4 text-muted-foreground" />
                      )}
                    </Button>
                  </div>
                  <Button
                    onClick={handleSaveToken}
                    disabled={setToken.isPending || !tokenInput.trim()}
                  >
                    {setToken.isPending ? (
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    ) : null}
                    {t('settings.github.token.buttons.save')}
                  </Button>
                </div>
                <p className="text-sm text-muted-foreground">
                  {t('settings.github.token.inputHelper')}
                </p>
              </div>

              <div className="relative">
                <div className="absolute inset-0 flex items-center">
                  <span className="w-full border-t" />
                </div>
                <div className="relative flex justify-center text-xs uppercase">
                  <span className="bg-background px-2 text-muted-foreground">
                    {t('settings.github.token.or')}
                  </span>
                </div>
              </div>

              <div className="space-y-2">
                <Button
                  variant="outline"
                  onClick={handleImportFromCli}
                  disabled={importToken.isPending}
                  className="w-full"
                >
                  {importToken.isPending ? (
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  ) : (
                    <Download className="h-4 w-4 mr-2" />
                  )}
                  {t('settings.github.token.buttons.import')}
                </Button>
                <p className="text-sm text-muted-foreground text-center">
                  {t('settings.github.token.importHelper')}
                </p>
              </div>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
