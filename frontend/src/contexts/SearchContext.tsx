import {
  createContext,
  useContext,
  useRef,
  useCallback,
  ReactNode,
} from 'react';
import { useLocation } from 'react-router-dom';

interface SearchState {
  active: boolean;
  focusInput: () => void;
  registerInputRef: (ref: HTMLInputElement | null) => void;
}

const SearchContext = createContext<SearchState | null>(null);

interface SearchProviderProps {
  children: ReactNode;
}

export function SearchProvider({ children }: SearchProviderProps) {
  const location = useLocation();
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Check if we're on a tasks route
  const isTasksRoute = /^\/projects\/[^/]+\/tasks/.test(location.pathname);

  const focusInput = useCallback(() => {
    if (inputRef.current && isTasksRoute) {
      inputRef.current.focus();
    }
  }, [isTasksRoute]);

  const registerInputRef = useCallback((ref: HTMLInputElement | null) => {
    inputRef.current = ref;
  }, []);

  const value: SearchState = {
    active: isTasksRoute,
    focusInput,
    registerInputRef,
  };

  return (
    <SearchContext.Provider value={value}>{children}</SearchContext.Provider>
  );
}

export function useSearch(): SearchState {
  const context = useContext(SearchContext);
  if (!context) {
    throw new Error('useSearch must be used within a SearchProvider');
  }
  return context;
}
