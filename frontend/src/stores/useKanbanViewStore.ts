import { create } from 'zustand';

const STORAGE_KEY = 'kanbanView.compact';

type State = {
  isCompact: boolean;
  toggleCompact: () => void;
};

export const useKanbanViewStore = create<State>((set) => ({
  isCompact: localStorage.getItem(STORAGE_KEY) === 'true',
  toggleCompact: () =>
    set((s) => {
      const next = !s.isCompact;
      try {
        localStorage.setItem(STORAGE_KEY, String(next));
      } catch {
        // Ignore storage errors
      }
      return { isCompact: next };
    }),
}));

export const useIsCompactView = () => useKanbanViewStore((s) => s.isCompact);
