import { create } from 'zustand';

const STORAGE_KEY = 'kanbanView.compact';

function loadCompact(): boolean {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === null) return false;
    return saved === 'true';
  } catch {
    return false;
  }
}

function saveCompact(value: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(value));
  } catch {
    // Ignore errors
  }
}

type State = {
  isCompact: boolean;
  toggleCompact: () => void;
  setCompact: (value: boolean) => void;
};

export const useKanbanViewStore = create<State>((set) => ({
  isCompact: loadCompact(),
  toggleCompact: () =>
    set((s) => {
      const next = !s.isCompact;
      saveCompact(next);
      return { isCompact: next };
    }),
  setCompact: (value) => {
    saveCompact(value);
    set({ isCompact: value });
  },
}));

export const useIsCompactView = () => useKanbanViewStore((s) => s.isCompact);
