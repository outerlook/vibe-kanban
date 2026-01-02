import { Command } from 'cmdk';
import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  CheckSquare,
  FolderOpen,
  LayoutGrid,
  Plus,
  Settings,
} from 'lucide-react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useProject } from '@/contexts/ProjectContext';
import { useProjects } from '@/hooks/useProjects';
import { useProjectTasks } from '@/hooks/useProjectTasks';
import { useKeyExit, useKeyOpenCommandPalette, Scope } from '@/keyboard';
import { openTaskForm } from '@/lib/openTaskForm';

export function CommandPalette() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const navigate = useNavigate();
  const { projects } = useProjects();
  const { projectId } = useProject();
  const { tasks } = useProjectTasks(projectId ?? '');
  const groupClassName =
    'py-1 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:text-muted-foreground';
  const itemClassName =
    'flex cursor-pointer items-center gap-2 rounded-md px-2 py-2 text-sm outline-none data-[selected]:bg-muted data-[selected]:text-foreground aria-selected:bg-muted aria-selected:text-foreground';
  const iconClassName = 'h-4 w-4 text-muted-foreground';

  useKeyOpenCommandPalette(
    () => setOpen(true),
    {
      scope: Scope.GLOBAL,
      enableOnFormTags: true,
      enableOnContentEditable: true,
      preventDefault: true,
      when: !open,
    }
  );

  useKeyExit(
    (event) => {
      event?.preventDefault();
      setOpen(false);
    },
    {
      scope: Scope.DIALOG,
      enableOnFormTags: true,
      enableOnContentEditable: true,
      preventDefault: true,
      when: open,
    }
  );

  useEffect(() => {
    if (!open) {
      setSearch('');
    }
  }, [open]);

  const runCommand = (callback: () => void) => {
    setOpen(false);
    callback();
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="p-0">
        <Command className="w-full">
          <Command.Input
            autoFocus
            placeholder="Search projects, tasks, commands..."
            value={search}
            onValueChange={setSearch}
            className="h-11 w-full border-b border-border bg-transparent px-3 text-sm outline-none placeholder:text-muted-foreground"
          />
          <Command.List className="max-h-[360px] overflow-y-auto p-2">
            <Command.Empty className="px-2 py-3 text-sm text-muted-foreground">
              No results found.
            </Command.Empty>

            {projects.length > 0 && (
              <Command.Group
                heading="Projects"
                className={groupClassName}
              >
                {projects.map((project) => (
                  <Command.Item
                    key={project.id}
                    value={`project ${project.name}`}
                    onSelect={() =>
                      runCommand(() =>
                        navigate(`/projects/${project.id}/tasks`)
                      )
                    }
                    className={itemClassName}
                  >
                    <FolderOpen className={iconClassName} />
                    {project.name}
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            {projectId && tasks.length > 0 && (
              <Command.Group
                heading="Tasks"
                className={groupClassName}
              >
                {tasks.map((task) => (
                  <Command.Item
                    key={task.id}
                    value={`task ${task.title}`}
                    onSelect={() =>
                      runCommand(() =>
                        navigate(`/projects/${projectId}/tasks/${task.id}`)
                      )
                    }
                    className={itemClassName}
                  >
                    <CheckSquare className={iconClassName} />
                    {task.title}
                  </Command.Item>
                ))}
              </Command.Group>
            )}

            <Command.Group
              heading="Commands"
              className={groupClassName}
            >
              <Command.Item
                value="command projects"
                onSelect={() => runCommand(() => navigate('/projects'))}
                className={itemClassName}
              >
                <LayoutGrid className={iconClassName} />
                Projects
              </Command.Item>
              <Command.Item
                value="command settings"
                onSelect={() => runCommand(() => navigate('/settings'))}
                className={itemClassName}
              >
                <Settings className={iconClassName} />
                Settings
              </Command.Item>
              {projectId && (
                <Command.Item
                  value="command create task"
                  onSelect={() =>
                    runCommand(() =>
                      openTaskForm({ mode: 'create', projectId })
                    )
                  }
                  className={itemClassName}
                >
                  <Plus className={iconClassName} />
                  Create Task
                </Command.Item>
              )}
            </Command.Group>
          </Command.List>
        </Command>
      </DialogContent>
    </Dialog>
  );
}
