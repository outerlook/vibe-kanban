import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQueryClient } from '@tanstack/react-query'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Switch } from '@/components/ui/switch'
import { Loader2, Server, Globe, CheckCircle2 } from 'lucide-react'
import {
  isTauriEnvironment,
  getServerMode,
  setServerMode,
  type ServerMode,
} from '@/lib/tauri-api'
import { refreshApiBaseUrl } from '@/lib/api'

/** Helper to check if mode is local */
function isLocalMode(mode: ServerMode): boolean {
  return mode.mode === 'local'
}

/** Helper to extract URL from remote mode */
function getRemoteUrlFromMode(mode: ServerMode): string {
  return mode.mode === 'remote' ? mode.url : ''
}

export function ServerModeSettings() {
  const { t } = useTranslation(['settings'])
  const queryClient = useQueryClient()

  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  const [mode, setMode] = useState<ServerMode>({ mode: 'local' })
  const [remoteUrlInput, setRemoteUrlInput] = useState('')
  const [urlError, setUrlError] = useState<string | null>(null)

  // Only render in Tauri environment
  const [isTauri, setIsTauri] = useState(false)

  useEffect(() => {
    setIsTauri(isTauriEnvironment())
  }, [])

  // Load current settings on mount
  useEffect(() => {
    if (!isTauri) return

    const loadSettings = async () => {
      setLoading(true)
      setError(null)
      try {
        const currentMode = await getServerMode()
        setMode(currentMode)
        // Pre-populate the URL input if in remote mode
        if (currentMode.mode === 'remote') {
          setRemoteUrlInput(currentMode.url)
        }
      } catch (err) {
        console.error('Failed to load server mode settings:', err)
        setError(t('settings.serverMode.loadError', { defaultValue: 'Failed to load settings' }))
      } finally {
        setLoading(false)
      }
    }

    loadSettings()
  }, [isTauri, t])

  const validateUrl = useCallback((url: string): string | null => {
    if (!url.trim()) {
      return t('settings.serverMode.remote.urlRequired', { defaultValue: 'Server URL is required' })
    }
    try {
      const parsed = new URL(url)
      if (!['http:', 'https:'].includes(parsed.protocol)) {
        return t('settings.serverMode.remote.urlInvalidProtocol', { defaultValue: 'URL must start with http:// or https://' })
      }
      return null
    } catch {
      return t('settings.serverMode.remote.urlInvalid', { defaultValue: 'Invalid URL format' })
    }
  }, [t])

  const handleModeChange = async (checked: boolean) => {
    // When switching to remote, validate URL first
    if (checked) {
      const validationError = validateUrl(remoteUrlInput)
      if (validationError) {
        setUrlError(validationError)
        return
      }
    }

    const newMode: ServerMode = checked
      ? { mode: 'remote', url: remoteUrlInput }
      : { mode: 'local' }

    setSaving(true)
    setError(null)
    setSuccess(false)

    try {
      await setServerMode(newMode)
      // Refresh the API base URL cache after mode change
      await refreshApiBaseUrl()
      // Invalidate all queries so they refetch with the new server URL
      await queryClient.invalidateQueries()
      setMode(newMode)
      setSuccess(true)
      setTimeout(() => setSuccess(false), 3000)
    } catch (err) {
      console.error('Failed to change server mode:', err)
      setError(t('settings.serverMode.saveError', { defaultValue: 'Failed to save settings' }))
    } finally {
      setSaving(false)
    }
  }

  const handleUrlChange = (value: string) => {
    setRemoteUrlInput(value)
    if (urlError) {
      setUrlError(validateUrl(value))
    }
  }

  const handleUrlBlur = () => {
    if (!isLocalMode(mode) || remoteUrlInput.trim()) {
      setUrlError(validateUrl(remoteUrlInput))
    }
  }

  const handleSaveUrl = async () => {
    const validationError = validateUrl(remoteUrlInput)
    if (validationError) {
      setUrlError(validationError)
      return
    }

    // Only save if we're already in remote mode
    if (mode.mode !== 'remote') {
      return
    }

    const newMode: ServerMode = { mode: 'remote', url: remoteUrlInput }

    setSaving(true)
    setError(null)
    setSuccess(false)

    try {
      await setServerMode(newMode)
      // Refresh the API base URL cache after URL change
      await refreshApiBaseUrl()
      // Invalidate all queries so they refetch with the new server URL
      await queryClient.invalidateQueries()
      setMode(newMode)
      setSuccess(true)
      setTimeout(() => setSuccess(false), 3000)
    } catch (err) {
      console.error('Failed to save remote URL:', err)
      setError(t('settings.serverMode.saveError', { defaultValue: 'Failed to save settings' }))
    } finally {
      setSaving(false)
    }
  }

  // Don't render if not in Tauri environment
  if (!isTauri) {
    return null
  }

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.serverMode.title', { defaultValue: 'Server Mode' })}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center py-4">
            <Loader2 className="h-6 w-6 animate-spin" />
          </div>
        </CardContent>
      </Card>
    )
  }

  const isLocal = isLocalMode(mode)

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('settings.serverMode.title', { defaultValue: 'Server Mode' })}</CardTitle>
        <CardDescription>
          {t('settings.serverMode.description', { defaultValue: 'Choose how the app connects to the backend server' })}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        {success && (
          <Alert variant="success">
            <AlertDescription className="flex items-center gap-2">
              <CheckCircle2 className="h-4 w-4" />
              {t('settings.serverMode.saveSuccess', { defaultValue: 'Settings saved' })}
            </AlertDescription>
          </Alert>
        )}

        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {isLocal ? (
              <Server className="h-5 w-5 text-muted-foreground" />
            ) : (
              <Globe className="h-5 w-5 text-muted-foreground" />
            )}
            <div className="space-y-0.5">
              <Label htmlFor="server-mode" className="text-base">
                {isLocal
                  ? t('settings.serverMode.local.label', { defaultValue: 'Local Mode' })
                  : t('settings.serverMode.remote.label', { defaultValue: 'Remote Mode' })}
              </Label>
              <p className="text-sm text-muted-foreground">
                {isLocal
                  ? t('settings.serverMode.local.description', { defaultValue: 'Server runs embedded within the app' })
                  : t('settings.serverMode.remote.description', { defaultValue: 'Connect to an external server' })}
              </p>
            </div>
          </div>
          <Switch
            id="server-mode"
            checked={!isLocal}
            onCheckedChange={handleModeChange}
            disabled={saving}
            aria-label={t('settings.serverMode.switchLabel', { defaultValue: 'Toggle server mode' })}
          />
        </div>

        {isLocal && (
          <div className="rounded-lg bg-muted/50 p-3">
            <div className="flex items-center gap-2 text-sm">
              <div className="h-2 w-2 rounded-full bg-green-500" />
              <span className="text-muted-foreground">
                {t('settings.serverMode.local.status', { defaultValue: 'Embedded server running' })}
              </span>
            </div>
          </div>
        )}

        {!isLocal && (
          <div className="space-y-3">
            <div className="space-y-2">
              <Label htmlFor="remote-url">
                {t('settings.serverMode.remote.urlLabel', { defaultValue: 'Server URL' })}
              </Label>
              <div className="flex gap-2">
                <Input
                  id="remote-url"
                  type="url"
                  placeholder={t('settings.serverMode.remote.urlPlaceholder', { defaultValue: 'https://your-server.example.com' })}
                  value={remoteUrlInput}
                  onChange={(e) => handleUrlChange(e.target.value)}
                  onBlur={handleUrlBlur}
                  aria-invalid={!!urlError}
                  className={urlError ? 'border-destructive' : undefined}
                />
                <Button
                  onClick={handleSaveUrl}
                  disabled={saving || !!urlError || remoteUrlInput === getRemoteUrlFromMode(mode)}
                  size="sm"
                >
                  {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                  {t('settings.serverMode.remote.save', { defaultValue: 'Save' })}
                </Button>
              </div>
              {urlError && (
                <p className="text-sm text-destructive">{urlError}</p>
              )}
              <p className="text-sm text-muted-foreground">
                {t('settings.serverMode.remote.urlHelper', { defaultValue: 'Enter the URL of the remote Vibe Kanban server' })}
              </p>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  )
}
