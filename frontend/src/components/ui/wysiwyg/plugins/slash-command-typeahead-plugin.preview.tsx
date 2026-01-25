/**
 * Preview component for SlashCommandTypeaheadPlugin
 *
 * This file demonstrates the dropdown states with mock data for visual verification.
 * Use ui-preview to verify: correct positioning, styling matches file-tag plugin.
 *
 * Usage with ui-preview:
 *   1. Import this component into a page/route
 *   2. Run the dev server
 *   3. Use ui-preview skill to take screenshots
 */

import { useState } from 'react';
import { Terminal } from 'lucide-react';

// Mock data matching the SkillInfo type
const mockSlashCommands = ['commit', 'review-pr', 'deploy', 'test'];

const mockSkills = [
  { name: 'commit', description: 'Create a git commit with staged changes', namespace: null },
  { name: 'review-pr', description: 'Review a pull request and provide feedback', namespace: null },
  { name: 'code-review', description: 'Perform a comprehensive code review', namespace: 'feature-dev' },
  { name: 'create-plugin', description: 'Guided end-to-end plugin creation workflow', namespace: 'plugin-dev' },
  { name: 'algorithmic-art', description: 'Creating algorithmic art using p5.js with seeded randomness', namespace: 'example-skills' },
];

type PreviewState = 'empty' | 'loading' | 'with-commands' | 'with-skills' | 'filtered' | 'no-results';

const MIN_WIDTH = 320;

function MockDropdown({ state, filter }: { state: PreviewState; filter?: string }) {
  // Filter the commands and skills based on state
  let displayCommands = mockSlashCommands;
  let displaySkills = mockSkills;

  if (state === 'filtered' && filter) {
    const lowerFilter = filter.toLowerCase();
    displayCommands = mockSlashCommands.filter((c) => c.startsWith(lowerFilter));
    displaySkills = mockSkills.filter((s) => s.name.startsWith(lowerFilter));
  }

  if (state === 'no-results') {
    displayCommands = [];
    displaySkills = [];
  }

  if (state === 'with-commands') {
    displaySkills = [];
  }

  if (state === 'with-skills') {
    displayCommands = [];
  }

  return (
    <div
      className="bg-background border border-border rounded-md shadow-lg overflow-y-auto"
      style={{ minWidth: MIN_WIDTH, maxHeight: 320 }}
    >
      {state === 'loading' ? (
        <div className="p-2 text-sm text-muted-foreground">Loading commands...</div>
      ) : displayCommands.length === 0 && displaySkills.length === 0 ? (
        <div className="p-2 text-sm text-muted-foreground">No commands available</div>
      ) : (
        <div className="py-1">
          {/* Slash Commands Section */}
          {displayCommands.length > 0 && (
            <>
              <div className="px-3 py-1 text-xs font-semibold text-muted-foreground uppercase">
                Commands
              </div>
              {displayCommands.map((name, index) => (
                <div
                  key={`cmd-${name}`}
                  className={`px-3 py-2 cursor-pointer text-sm ${
                    index === 0 ? 'bg-muted text-foreground' : 'hover:bg-muted'
                  }`}
                >
                  <div className="flex items-center gap-2 font-medium">
                    <Terminal className="h-3.5 w-3.5 text-blue-600 flex-shrink-0" />
                    <span>/{name}</span>
                  </div>
                </div>
              ))}
            </>
          )}

          {/* Skills Section */}
          {displaySkills.length > 0 && (
            <>
              {displayCommands.length > 0 && <div className="border-t my-1" />}
              <div className="px-3 py-1 text-xs font-semibold text-muted-foreground uppercase">
                Skills
              </div>
              {displaySkills.map((skill, idx) => {
                const displayName = skill.namespace
                  ? `/${skill.namespace}:${skill.name}`
                  : `/${skill.name}`;
                const index = displayCommands.length + idx;
                return (
                  <div
                    key={`skill-${displayName}`}
                    className={`px-3 py-2 cursor-pointer text-sm ${
                      index === 0 && displayCommands.length === 0
                        ? 'bg-muted text-foreground'
                        : 'hover:bg-muted'
                    }`}
                  >
                    <div className="flex items-center gap-2 font-medium">
                      <Terminal className="h-3.5 w-3.5 text-green-600 flex-shrink-0" />
                      <span>{displayName}</span>
                    </div>
                    {skill.description && (
                      <div className="text-xs text-muted-foreground mt-0.5 truncate">
                        {skill.description.slice(0, 80)}
                        {skill.description.length > 80 ? '...' : ''}
                      </div>
                    )}
                  </div>
                );
              })}
            </>
          )}
        </div>
      )}
    </div>
  );
}

export function SlashCommandTypeaheadPreview() {
  const [selectedState, setSelectedState] = useState<PreviewState>('with-commands');
  const [filter, setFilter] = useState('com');

  const states: { value: PreviewState; label: string }[] = [
    { value: 'loading', label: 'Loading' },
    { value: 'with-commands', label: 'Commands Only' },
    { value: 'with-skills', label: 'Skills Only' },
    { value: 'filtered', label: 'Filtered (type filter below)' },
    { value: 'no-results', label: 'No Results' },
  ];

  return (
    <div className="p-8 space-y-8">
      <div>
        <h1 className="text-2xl font-bold mb-2">SlashCommandTypeaheadPlugin Preview</h1>
        <p className="text-muted-foreground mb-4">
          Visual preview of the slash command typeahead dropdown states.
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

        {selectedState === 'filtered' && (
          <div>
            <label className="text-sm text-muted-foreground">Filter text:</label>
            <input
              type="text"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="ml-2 px-2 py-1 border rounded text-sm"
              placeholder="e.g., com"
            />
          </div>
        )}
      </div>

      <div className="space-y-2">
        <h2 className="text-lg font-semibold">Dropdown Preview</h2>
        <p className="text-sm text-muted-foreground mb-4">
          State: <code className="bg-muted px-1 rounded">{selectedState}</code>
          {selectedState === 'filtered' && (
            <span>
              {' '}
              | Filter: <code className="bg-muted px-1 rounded">{filter || '(empty)'}</code>
            </span>
          )}
        </p>
        <MockDropdown state={selectedState} filter={filter} />
      </div>

      <div className="space-y-4">
        <h2 className="text-lg font-semibold">All States Side by Side</h2>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <h3 className="text-sm font-medium mb-2">Loading State</h3>
            <MockDropdown state="loading" />
          </div>
          <div>
            <h3 className="text-sm font-medium mb-2">No Results</h3>
            <MockDropdown state="no-results" />
          </div>
          <div>
            <h3 className="text-sm font-medium mb-2">Commands + Skills (full list)</h3>
            <MockDropdown state="empty" />
          </div>
          <div>
            <h3 className="text-sm font-medium mb-2">Filtered: &quot;com&quot;</h3>
            <MockDropdown state="filtered" filter="com" />
          </div>
        </div>
      </div>
    </div>
  );
}

export default SlashCommandTypeaheadPreview;
