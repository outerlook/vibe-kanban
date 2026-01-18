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
import { Loader2, Server, CheckCircle2 } from 'lucide-react'
import { isTauriEnvironment, getServerUrl, setServerUrl } from '@/lib/tauri-api'
import { refreshApiBaseUrl } from '@/lib/api'

export function ServerModeSettings() {
  const { t } = useTranslation(['settings'])
  const queryClient = useQueryClient()

  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  const [currentUrl, setCurrentUrl] = useState('')
  const [useCustomUrl, setUseCustomUrl] = useState(false)
  const [customUrlInput, setCustomUrlInput] = useState('')
  const [urlError, setUrlError] = useState<string | null>(null)

  const [isTauri, setIsTauri] = useState(false)

  useEffect(() => {
    setIsTauri(isTauriEnvironment())
  }, [])

  useEffect(() => {
    if (!isTauri) return

    const loadSettings = async () => {
      setLoading(true)
      setError(null)
      try {
        const url = await getServerUrl()
        setCurrentUrl(url)
        // Default to auto-discovery mode (toggle off)
        // The user can enable custom URL mode to override
        setUseCustomUrl(false)
        setCustomUrlInput(url)
      } catch (err) {
        console.error('Failed to load server URL settings:', err)
        setError(
          t('settings.serverUrl.loadError', {
            defaultValue: 'Failed to load settings',
          })
        )
      } finally {
        setLoading(false)
      }
    }

    loadSettings()
  }, [isTauri, t])

  const validateUrl = useCallback(
    (url: string): string | null => {
      if (!url.trim()) {
        return t('settings.serverUrl.urlRequired', {
          defaultValue: 'Server URL is required',
        })
      }
      try {
        const parsed = new URL(url)
        if (!['http:', 'https:'].includes(parsed.protocol)) {
          return t('settings.serverUrl.urlInvalidProtocol', {
            defaultValue: 'URL must start with http:// or https://',
          })
        }
        return null
      } catch {
        return t('settings.serverUrl.urlInvalid', {
          defaultValue: 'Invalid URL format',
        })
      }
    },
    [t]
  )

  const handleToggleCustomUrl = async (checked: boolean) => {
    if (checked) {
      // Switching to custom mode - just enable the input, don't save yet
      setUseCustomUrl(true)
      setUrlError(null)
    } else {
      // Switching to auto-discovery - save null to clear custom URL
      setSaving(true)
      setError(null)
      setSuccess(false)

      try {
        await setServerUrl(null)
        await refreshApiBaseUrl()
        await queryClient.invalidateQueries()
        // Fetch the new auto-discovered URL
        const newUrl = await getServerUrl()
        setCurrentUrl(newUrl)
        setCustomUrlInput(newUrl)
        setUseCustomUrl(false)
        setUrlError(null)
        setSuccess(true)
        setTimeout(() => setSuccess(false), 3000)
      } catch (err) {
        console.error('Failed to switch to auto-discovery:', err)
        setError(
          t('settings.serverUrl.saveError', {
            defaultValue: 'Failed to save settings',
          })
        )
      } finally {
        setSaving(false)
      }
    }
  }

  const handleUrlChange = (value: string) => {
    setCustomUrlInput(value)
    if (urlError) {
      setUrlError(validateUrl(value))
    }
  }

  const handleUrlBlur = () => {
    if (customUrlInput.trim()) {
      setUrlError(validateUrl(customUrlInput))
    }
  }

  const handleSaveCustomUrl = async () => {
    const validationError = validateUrl(customUrlInput)
    if (validationError) {
      setUrlError(validationError)
      return
    }

    setSaving(true)
    setError(null)
    setSuccess(false)

    try {
      await setServerUrl(customUrlInput)
      await refreshApiBaseUrl()
      await queryClient.invalidateQueries()
      const newUrl = await getServerUrl()
      setCurrentUrl(newUrl)
      setSuccess(true)
      setTimeout(() => setSuccess(false), 3000)
    } catch (err) {
      console.error('Failed to save custom URL:', err)
      setError(
        t('settings.serverUrl.saveError', {
          defaultValue: 'Failed to save settings',
        })
      )
    } finally {
      setSaving(false)
    }
  }

  if (!isTauri) {
    return null
  }

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>
            {t('settings.serverUrl.title', { defaultValue: 'Server URL' })}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center py-4">
            <Loader2 className="h-6 w-6 animate-spin" />
          </div>
        </CardContent>
      </Card>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {t('settings.serverUrl.title', { defaultValue: 'Server URL' })}
        </CardTitle>
        <CardDescription>
          {t('settings.serverUrl.description', {
            defaultValue: 'Configure how the app connects to the backend server',
          })}
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
              {t('settings.serverUrl.saveSuccess', {
                defaultValue: 'Settings saved',
              })}
            </AlertDescription>
          </Alert>
        )}

        <div className="rounded-lg bg-muted/50 p-3">
          <div className="flex items-center gap-2 text-sm">
            <Server className="h-4 w-4 text-muted-foreground" />
            <span className="text-muted-foreground">
              {t('settings.serverUrl.currentUrl', {
                defaultValue: 'Current server:',
              })}
            </span>
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
              {currentUrl}
            </code>
          </div>
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="use-custom-url" className="text-base">
              {t('settings.serverUrl.useCustom', {
                defaultValue: 'Use custom server URL',
              })}
            </Label>
            <p className="text-sm text-muted-foreground">
              {useCustomUrl
                ? t('settings.serverUrl.customDescription', {
                    defaultValue: 'Connect to a specific server URL',
                  })
                : t('settings.serverUrl.autoDescription', {
                    defaultValue:
                      'Automatically discover the embedded server URL',
                  })}
            </p>
          </div>
          <Switch
            id="use-custom-url"
            checked={useCustomUrl}
            onCheckedChange={handleToggleCustomUrl}
            disabled={saving}
            aria-label={t('settings.serverUrl.toggleLabel', {
              defaultValue: 'Toggle custom URL mode',
            })}
          />
        </div>

        {useCustomUrl && (
          <div className="space-y-3">
            <div className="space-y-2">
              <Label htmlFor="custom-url">
                {t('settings.serverUrl.urlLabel', {
                  defaultValue: 'Custom Server URL',
                })}
              </Label>
              <div className="flex gap-2">
                <Input
                  id="custom-url"
                  type="url"
                  placeholder={t('settings.serverUrl.urlPlaceholder', {
                    defaultValue: 'https://your-server.example.com',
                  })}
                  value={customUrlInput}
                  onChange={(e) => handleUrlChange(e.target.value)}
                  onBlur={handleUrlBlur}
                  aria-invalid={!!urlError}
                  className={urlError ? 'border-destructive' : undefined}
                />
                <Button
                  onClick={handleSaveCustomUrl}
                  disabled={
                    saving || !!urlError || customUrlInput === currentUrl
                  }
                  size="sm"
                >
                  {saving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                  {t('settings.serverUrl.save', { defaultValue: 'Save' })}
                </Button>
              </div>
              {urlError && <p className="text-sm text-destructive">{urlError}</p>}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  )
}
