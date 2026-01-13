import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { CustomEditorDialog } from '@/components/dialogs/settings/CustomEditorDialog';
import { ConfirmDialog } from '@/components/dialogs';
import { EditorAvailabilityIndicator } from '@/components/EditorAvailabilityIndicator';
import { useCustomEditors, useDeleteCustomEditor } from '@/hooks';
import type { CustomEditorWithAvailability } from '@/hooks/useCustomEditors';
import { Code2, Edit2, Loader2, Trash2 } from 'lucide-react';

type DialogMode = 'create' | 'edit';

export function CustomEditorsList() {
  const { t } = useTranslation('settings');
  const { data, isLoading, error } = useCustomEditors();
  const deleteEditor = useDeleteCustomEditor();
  const [dialogMode, setDialogMode] = useState<DialogMode>('create');
  const [dialogOpen, setDialogOpen] = useState(false);
  const [selectedEditor, setSelectedEditor] =
    useState<CustomEditorWithAvailability | null>(null);

  const editors = data ?? [];

  const handleCloseDialog = () => {
    setDialogOpen(false);
    setSelectedEditor(null);
    setDialogMode('create');
  };

  const handleCreate = () => {
    setSelectedEditor(null);
    setDialogMode('create');
    setDialogOpen(true);
  };

  const handleEdit = (editor: CustomEditorWithAvailability) => {
    setSelectedEditor(editor);
    setDialogMode('edit');
    setDialogOpen(true);
  };

  const handleDelete = async (editor: CustomEditorWithAvailability) => {
    const result = await ConfirmDialog.show({
      title: t('settings.general.customEditors.deleteConfirm.title'),
      message: t('settings.general.customEditors.deleteConfirm.message', {
        name: editor.name,
      }),
      confirmText: t('settings.general.customEditors.deleteConfirm.confirm'),
      variant: 'destructive',
    });

    if (result !== 'confirmed') {
      return;
    }

    await deleteEditor.mutateAsync(editor.id);
  };

  return (
    <>
      {error && (
        <Alert variant="destructive">
          <AlertDescription>
            {error instanceof Error
              ? error.message
              : t('settings.general.customEditors.loadError')}
          </AlertDescription>
        </Alert>
      )}

      {isLoading ? (
        <div className="flex items-center justify-center py-8">
          <Loader2 className="h-8 w-8 animate-spin" />
          <span className="ml-2">
            {t('settings.general.customEditors.loading')}
          </span>
        </div>
      ) : editors.length === 0 ? (
        <div className="flex flex-col items-center justify-center gap-2 py-8 text-muted-foreground">
          <Code2 className="h-8 w-8" />
          <p>{t('settings.general.customEditors.empty')}</p>
        </div>
      ) : (
        <div className="border rounded-lg overflow-hidden">
          <div className="max-h-[400px] overflow-auto">
            <table className="w-full">
              <thead className="border-b bg-muted/50 sticky top-0">
                <tr>
                  <th className="text-left p-2 text-sm font-medium">
                    {t('settings.general.customEditors.table.name')}
                  </th>
                  <th className="text-left p-2 text-sm font-medium">
                    {t('settings.general.customEditors.table.command')}
                  </th>
                  <th className="text-left p-2 text-sm font-medium">
                    {t('settings.general.customEditors.table.availability')}
                  </th>
                  <th className="text-right p-2 text-sm font-medium">
                    {t('settings.general.customEditors.table.actions')}
                  </th>
                </tr>
              </thead>
              <tbody>
                {editors.map((editor) => (
                  <tr
                    key={editor.id}
                    className="border-b hover:bg-muted/30 transition-colors"
                  >
                    <td className="p-2 text-sm font-medium">
                      <div className="flex items-center gap-2">
                        <Code2 className="h-4 w-4 text-muted-foreground" />
                        <span>{editor.name}</span>
                      </div>
                    </td>
                    <td className="p-2 text-sm">
                      <div
                        className="max-w-[360px] truncate font-mono"
                        title={editor.command}
                      >
                        {editor.command}
                      </div>
                    </td>
                    <td className="p-2 text-sm">
                      <EditorAvailabilityIndicator
                        availability={
                          editor.available ? 'available' : 'unavailable'
                        }
                      />
                    </td>
                    <td className="p-2">
                      <div className="flex justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => handleEdit(editor)}
                          title={t('settings.general.customEditors.actions.edit')}
                        >
                          <Edit2 className="h-3 w-3" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => handleDelete(editor)}
                          title={t(
                            'settings.general.customEditors.actions.delete'
                          )}
                          disabled={deleteEditor.isPending}
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      <div className="flex justify-end">
        <Button onClick={handleCreate}>
          <Code2 className="h-4 w-4 mr-2" />
          {t('settings.general.customEditors.actions.add')}
        </Button>
      </div>

      <CustomEditorDialog
        open={dialogOpen}
        mode={dialogMode}
        editor={selectedEditor || undefined}
        onClose={handleCloseDialog}
      />
    </>
  );
}
