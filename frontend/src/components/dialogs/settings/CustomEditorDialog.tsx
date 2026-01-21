import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2 } from 'lucide-react';
import { useCreateCustomEditor, useUpdateCustomEditor } from '@/hooks';
import { getErrorMessage } from '@/lib/modals';
import type { CustomEditorResponse } from 'shared/types';

export interface CustomEditorDialogProps {
  open: boolean;
  mode: 'create' | 'edit';
  editor?: CustomEditorResponse;
  onClose: () => void;
}

const DEFAULT_ARGUMENT = '%d';

export function CustomEditorDialog({
  open,
  mode,
  editor,
  onClose,
}: CustomEditorDialogProps) {
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [argument, setArgument] = useState(DEFAULT_ARGUMENT);
  const [error, setError] = useState<string | null>(null);
  const createEditor = useCreateCustomEditor();
  const updateEditor = useUpdateCustomEditor();
  const isSaving = createEditor.isPending || updateEditor.isPending;
  const isEditMode = mode === 'edit';

  useEffect(() => {
    if (!open) {
      return;
    }

    if (isEditMode && editor) {
      setName(editor.name);
      setCommand(editor.command);
      setArgument(editor.argument);
    } else {
      setName('');
      setCommand('');
      setArgument(DEFAULT_ARGUMENT);
    }

    setError(null);
  }, [open, isEditMode, editor]);

  const trimmedName = name.trim();
  const trimmedCommand = command.trim();
  const isValid = trimmedName.length > 0 && trimmedCommand.length > 0;
  const canSubmit = isValid && (!isEditMode || Boolean(editor));

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    if (!isValid) {
      setError('Name and command are required.');
      return;
    }

    if (isEditMode && !editor) {
      setError('No custom editor selected.');
      return;
    }

    setError(null);

    try {
      const trimmedArgument = argument.trim() || null;
      if (isEditMode && editor) {
        await updateEditor.mutateAsync({
          id: editor.id,
          name: trimmedName,
          command: trimmedCommand,
          argument: trimmedArgument,
        });
      } else {
        await createEditor.mutateAsync({
          name: trimmedName,
          command: trimmedCommand,
          argument: trimmedArgument,
        });
      }

      onClose();
    } catch (err: unknown) {
      setError(getErrorMessage(err) || 'Failed to save custom editor.');
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && !isSaving) {
          onClose();
        }
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {isEditMode ? 'Edit Custom Editor' : 'Create Custom Editor'}
          </DialogTitle>
        </DialogHeader>

        <form className="space-y-4" onSubmit={handleSubmit}>
          <div className="space-y-2">
            <Label htmlFor="custom-editor-name">Name</Label>
            <Input
              id="custom-editor-name"
              value={name}
              onChange={(event) => {
                setName(event.target.value);
                setError(null);
              }}
              placeholder="e.g., VS Code"
              disabled={isSaving}
              autoFocus
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="custom-editor-command">Command</Label>
            <Input
              id="custom-editor-command"
              value={command}
              onChange={(event) => {
                setCommand(event.target.value);
                setError(null);
              }}
              placeholder="e.g., code --folder-uri"
              disabled={isSaving}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="custom-editor-argument">Arguments</Label>
            <Input
              id="custom-editor-argument"
              value={argument}
              onChange={(event) => {
                setArgument(event.target.value);
                setError(null);
              }}
              placeholder="%d"
              disabled={isSaving}
            />
            <p className="text-xs text-muted-foreground">
              Use %d for directory path, %f for file path
            </p>
          </div>

          {error && (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={onClose}
              disabled={isSaving}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSaving || !canSubmit}>
              {isSaving && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {isEditMode ? 'Save Changes' : 'Create Editor'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
