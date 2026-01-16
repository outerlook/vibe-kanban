import { useEffect, useMemo, useState } from 'react';
import { Code2 } from 'lucide-react';
import { EditorType } from 'shared/types';
import {
  SplitButton,
  type SplitButtonOption,
} from '@/components/ui/split-button';
import { useUserSystem } from '@/components/ConfigProvider';
import { useCustomEditors } from '@/hooks';
import { IdeIcon, getIdeName } from './IdeIcon';

const PREFERRED_EDITOR_KEY = 'preferredEditor';
const CUSTOM_EDITOR_PREFIX = 'custom:';

type OpenInIdeSplitButtonProps = {
  onClick: (editorId: string) => void;
  disabled?: boolean;
  className?: string;
};

const getStoredPreference = (): string | null => {
  if (typeof window === 'undefined') return null;
  return localStorage.getItem(PREFERRED_EDITOR_KEY);
};

const isCustomEditorValue = (value: string) =>
  value.startsWith(CUSTOM_EDITOR_PREFIX);

export function OpenInIdeSplitButton({
  onClick,
  disabled = false,
  className,
}: OpenInIdeSplitButtonProps) {
  const { config } = useUserSystem();
  const { data: customEditors = [], isLoading: customEditorsLoading } =
    useCustomEditors();
  const [selectedEditor, setSelectedEditor] = useState<string>(
    () => getStoredPreference() ?? ''
  );

  const builtInOptions = useMemo<SplitButtonOption<string>[]>(
    () =>
      Object.values(EditorType).map((editorType) => ({
        value: editorType,
        label: getIdeName(editorType),
        icon: <IdeIcon editorType={editorType} className="h-3.5 w-3.5" />,
      })),
    []
  );

  const customOptions = useMemo<SplitButtonOption<string>[]>(
    () =>
      customEditors.map((editor) => ({
        value: `${CUSTOM_EDITOR_PREFIX}${editor.id}`,
        label: editor.name,
        icon: <Code2 className="h-3.5 w-3.5" />,
      })),
    [customEditors]
  );

  const preferredFromConfig = useMemo(() => {
    const editorType = config?.editor?.editor_type ?? null;
    if (!editorType) return null;
    if (editorType === EditorType.CUSTOM && config?.editor?.custom_editor_id) {
      return `${CUSTOM_EDITOR_PREFIX}${config.editor.custom_editor_id}`;
    }
    return editorType;
  }, [config?.editor?.editor_type, config?.editor?.custom_editor_id]);

  const baseOptions = useMemo(
    () => [...builtInOptions, ...customOptions],
    [builtInOptions, customOptions]
  );

  const missingCustomSelection = useMemo(() => {
    if (!selectedEditor || !isCustomEditorValue(selectedEditor)) return null;
    if (baseOptions.some((option) => option.value === selectedEditor)) {
      return null;
    }
    if (!customEditorsLoading) return null;
    return {
      value: selectedEditor,
      label: 'Custom Editor',
      icon: <Code2 className="h-3.5 w-3.5" />,
    };
  }, [baseOptions, customEditorsLoading, selectedEditor]);

  const options = useMemo(
    () =>
      missingCustomSelection
        ? [...baseOptions, missingCustomSelection]
        : baseOptions,
    [baseOptions, missingCustomSelection]
  );

  const selectedOption = useMemo(
    () => options.find((option) => option.value === selectedEditor),
    [options, selectedEditor]
  );

  useEffect(() => {
    if (!options.length) return;

    if (!selectedEditor) {
      const fallback = preferredFromConfig ?? options[0]?.value ?? '';
      if (fallback) {
        setSelectedEditor(fallback);
        localStorage.setItem(PREFERRED_EDITOR_KEY, fallback);
      }
      return;
    }

    const hasSelection = options.some(
      (option) => option.value === selectedEditor
    );

    if (
      !hasSelection &&
      !(customEditorsLoading && isCustomEditorValue(selectedEditor))
    ) {
      const fallback = preferredFromConfig ?? options[0]?.value ?? '';
      if (fallback && fallback !== selectedEditor) {
        setSelectedEditor(fallback);
        localStorage.setItem(PREFERRED_EDITOR_KEY, fallback);
      }
    }
  }, [customEditorsLoading, options, preferredFromConfig, selectedEditor]);

  const handleSelect = (value: string) => {
    setSelectedEditor(value);
    localStorage.setItem(PREFERRED_EDITOR_KEY, value);
    onClick(value);
  };

  const handlePrimaryClick = (value: string) => {
    onClick(value);
  };

  return (
    <SplitButton
      options={options}
      selectedValue={selectedEditor}
      onSelect={handleSelect}
      onPrimaryClick={handlePrimaryClick}
      disabled={disabled}
      className={className}
      icon={selectedOption?.icon}
    />
  );
}
