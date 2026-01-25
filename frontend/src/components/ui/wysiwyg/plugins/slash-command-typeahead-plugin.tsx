import { useState, useCallback, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import {
  LexicalTypeaheadMenuPlugin,
  MenuOption,
} from '@lexical/react/LexicalTypeaheadMenuPlugin';
import { $createTextNode } from 'lexical';
import { Terminal } from 'lucide-react';
import { skillsApi } from '@/lib/api';
import type { SkillInfo } from 'shared/types';

type SlashCommandItem =
  | { type: 'slash_command'; name: string }
  | { type: 'skill'; skill: SkillInfo };

class SlashCommandOption extends MenuOption {
  item: SlashCommandItem;

  constructor(item: SlashCommandItem) {
    const key =
      item.type === 'slash_command'
        ? `cmd-${item.name}`
        : `skill-${item.skill.namespace ? `${item.skill.namespace}:` : ''}${item.skill.name}`;
    super(key);
    this.item = item;
  }
}

const MAX_DIALOG_HEIGHT = 320;
const VIEWPORT_MARGIN = 8;
const VERTICAL_GAP = 4;
const VERTICAL_GAP_ABOVE = 24;
const MIN_WIDTH = 320;

function getMenuPosition(anchorEl: HTMLElement) {
  const rect = anchorEl.getBoundingClientRect();
  const viewportHeight = window.innerHeight;
  const viewportWidth = window.innerWidth;

  const spaceAbove = rect.top;
  const spaceBelow = viewportHeight - rect.bottom;

  const showBelow = spaceBelow >= spaceAbove;

  const availableVerticalSpace = showBelow ? spaceBelow : spaceAbove;

  const maxHeight = Math.max(
    0,
    Math.min(MAX_DIALOG_HEIGHT, availableVerticalSpace - 2 * VIEWPORT_MARGIN)
  );

  let top: number | undefined;
  let bottom: number | undefined;

  if (showBelow) {
    top = rect.bottom + VERTICAL_GAP;
  } else {
    bottom = viewportHeight - rect.top + VERTICAL_GAP_ABOVE;
  }

  let left = rect.left;
  const maxLeft = viewportWidth - MIN_WIDTH - VIEWPORT_MARGIN;
  if (left > maxLeft) {
    left = Math.max(VIEWPORT_MARGIN, maxLeft);
  }

  return { top, bottom, left, maxHeight };
}

function getDisplayName(item: SlashCommandItem): string {
  if (item.type === 'slash_command') {
    return `/${item.name}`;
  }
  const skill = item.skill;
  if (skill.namespace) {
    return `/${skill.namespace}:${skill.name}`;
  }
  return `/${skill.name}`;
}

function getDescription(item: SlashCommandItem): string | null {
  if (item.type === 'slash_command') {
    return null;
  }
  return item.skill.description;
}

export function SlashCommandTypeaheadPlugin() {
  const [editor] = useLexicalComposerContext();
  const [options, setOptions] = useState<SlashCommandOption[]>([]);
  const [allItems, setAllItems] = useState<SlashCommandItem[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const itemRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const lastSelectedIndexRef = useRef<number>(-1);
  const fetchedRef = useRef(false);

  // Fetch skills data once
  useEffect(() => {
    if (fetchedRef.current) return;
    fetchedRef.current = true;

    setIsLoading(true);
    skillsApi
      .getSkills()
      .then((data) => {
        const items: SlashCommandItem[] = [
          ...data.slash_commands.map(
            (name): SlashCommandItem => ({ type: 'slash_command', name })
          ),
          ...data.skills.map(
            (skill): SlashCommandItem => ({ type: 'skill', skill })
          ),
        ];
        setAllItems(items);
      })
      .catch((err) => {
        console.error('Failed to fetch skills', err);
      })
      .finally(() => {
        setIsLoading(false);
      });
  }, []);

  const onQueryChange = useCallback(
    (query: string | null) => {
      // Lexical uses null to indicate "no active query / close menu"
      if (query === null) {
        setOptions([]);
        return;
      }

      // Filter items based on query
      const lowerQuery = query.toLowerCase();
      const filtered = allItems.filter((item) => {
        const displayName = getDisplayName(item).toLowerCase();
        // Remove the leading / for matching
        const nameWithoutSlash = displayName.slice(1);
        return nameWithoutSlash.startsWith(lowerQuery);
      });

      setOptions(filtered.map((item) => new SlashCommandOption(item)));
    },
    [allItems]
  );

  return (
    <LexicalTypeaheadMenuPlugin<SlashCommandOption>
      triggerFn={(text) => {
        // Match / at start of line or after whitespace, followed by optional command characters
        const match = /(?:^|\s)\/([^\s/]*)$/.exec(text);
        if (!match) return null;
        const offset = match.index + match[0].indexOf('/');
        return {
          leadOffset: offset,
          matchingString: match[1],
          replaceableString: match[0].slice(match[0].indexOf('/')),
        };
      }}
      options={options}
      onQueryChange={onQueryChange}
      onSelectOption={(option, nodeToReplace, closeMenu) => {
        editor.update(() => {
          const textToInsert = getDisplayName(option.item) + ' ';

          if (!nodeToReplace) return;

          // Create the node we want to insert
          const textNode = $createTextNode(textToInsert);

          // Replace the trigger text (e.g., "/com") with selected content
          nodeToReplace.replace(textNode);

          // Move the cursor to the end of the inserted text
          textNode.select(textToInsert.length, textToInsert.length);
        });

        closeMenu();
      }}
      menuRenderFn={(
        anchorRef,
        { selectedIndex, selectOptionAndCleanUp, setHighlightedIndex }
      ) => {
        if (!anchorRef.current) return null;

        const { top, bottom, left, maxHeight } = getMenuPosition(
          anchorRef.current
        );

        // Scroll selected item into view when navigating with arrow keys
        if (
          selectedIndex !== null &&
          selectedIndex !== lastSelectedIndexRef.current
        ) {
          lastSelectedIndexRef.current = selectedIndex;
          setTimeout(() => {
            const itemEl = itemRefs.current.get(selectedIndex);
            if (itemEl) {
              itemEl.scrollIntoView({ block: 'nearest' });
            }
          }, 0);
        }

        const slashCommands = options.filter(
          (o) => o.item.type === 'slash_command'
        );
        const skills = options.filter((o) => o.item.type === 'skill');

        return createPortal(
          <div
            className="fixed bg-background border border-border rounded-md shadow-lg overflow-y-auto"
            style={{
              top,
              bottom,
              left,
              maxHeight,
              minWidth: MIN_WIDTH,
              zIndex: 10000,
            }}
          >
            {isLoading ? (
              <div className="p-2 text-sm text-muted-foreground">
                Loading commands...
              </div>
            ) : options.length === 0 ? (
              <div className="p-2 text-sm text-muted-foreground">
                No commands available
              </div>
            ) : (
              <div className="py-1">
                {/* Slash Commands Section */}
                {slashCommands.length > 0 && (
                  <>
                    <div className="px-3 py-1 text-xs font-semibold text-muted-foreground uppercase">
                      Commands
                    </div>
                    {slashCommands.map((option) => {
                      const index = options.indexOf(option);
                      return (
                        <div
                          key={option.key}
                          ref={(el) => {
                            if (el) itemRefs.current.set(index, el);
                            else itemRefs.current.delete(index);
                          }}
                          className={`px-3 py-2 cursor-pointer text-sm ${
                            index === selectedIndex
                              ? 'bg-muted text-foreground'
                              : 'hover:bg-muted'
                          }`}
                          onMouseEnter={() => setHighlightedIndex(index)}
                          onClick={() => selectOptionAndCleanUp(option)}
                        >
                          <div className="flex items-center gap-2 font-medium">
                            <Terminal className="h-3.5 w-3.5 text-blue-600 flex-shrink-0" />
                            <span>{getDisplayName(option.item)}</span>
                          </div>
                        </div>
                      );
                    })}
                  </>
                )}

                {/* Skills Section */}
                {skills.length > 0 && (
                  <>
                    {slashCommands.length > 0 && (
                      <div className="border-t my-1" />
                    )}
                    <div className="px-3 py-1 text-xs font-semibold text-muted-foreground uppercase">
                      Skills
                    </div>
                    {skills.map((option) => {
                      const index = options.indexOf(option);
                      const description = getDescription(option.item);
                      return (
                        <div
                          key={option.key}
                          ref={(el) => {
                            if (el) itemRefs.current.set(index, el);
                            else itemRefs.current.delete(index);
                          }}
                          className={`px-3 py-2 cursor-pointer text-sm ${
                            index === selectedIndex
                              ? 'bg-muted text-foreground'
                              : 'hover:bg-muted'
                          }`}
                          onMouseEnter={() => setHighlightedIndex(index)}
                          onClick={() => selectOptionAndCleanUp(option)}
                        >
                          <div className="flex items-center gap-2 font-medium">
                            <Terminal className="h-3.5 w-3.5 text-green-600 flex-shrink-0" />
                            <span>{getDisplayName(option.item)}</span>
                          </div>
                          {description && (
                            <div className="text-xs text-muted-foreground mt-0.5 truncate">
                              {description.slice(0, 80)}
                              {description.length > 80 ? '...' : ''}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </>
                )}
              </div>
            )}
          </div>,
          document.body
        );
      }}
    />
  );
}
