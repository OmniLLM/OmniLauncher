import { useCallback, useRef, useState } from "react";
import {
  isAiPrefix,
  isSlashPrefix,
  isPluginManagerQuery,
  isConversationResetCommand,
  isHelpQuery,
  isHelpHintQuery,
  slashSuggestions,
  helpResults,
} from "../launcherConfig";
import { invoke } from "../lib/runtime";
import { pluginManagerResult } from "../features/launcher/pluginManagerResult";
import { SEARCH_DEBOUNCE_MS } from "../features/launcher/constants";
import type { QueryResult } from "../types/app";

export interface UseSearchArgs {
  aiModeEnabled: boolean;
  setAiModeEnabled: React.Dispatch<React.SetStateAction<boolean>>;
  setHistoryIdx: React.Dispatch<React.SetStateAction<number>>;
}

export interface UseSearchResult {
  query: string;
  setQuery: React.Dispatch<React.SetStateAction<string>>;
  results: QueryResult[];
  setResults: React.Dispatch<React.SetStateAction<QueryResult[]>>;
  searching: boolean;
  searchError: string | null;
  searchSeqRef: React.MutableRefObject<number>;
  debounceRef: React.MutableRefObject<ReturnType<typeof setTimeout> | null>;
  doSearch: (q: string) => Promise<void>;
  handleQueryChange: (value: string) => void;
}

export function useSearch(args: UseSearchArgs): UseSearchResult {
  const { aiModeEnabled, setAiModeEnabled, setHistoryIdx } = args;

  const [query, setQuery] = useState("");
  const [results, setResults] = useState<QueryResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  // Monotonic request id — stale plugin responses (slower than a newer
  // keystroke's request) get dropped instead of clobbering fresh results.
  const searchSeqRef = useRef(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const doSearch = useCallback(async (q: string) => {
    setSearchError(null);
    if (isHelpQuery(q)) {
      setResults(helpResults());
      setSearching(false);
      return;
    }

    if (isConversationResetCommand(q)) {
      setResults([]);
      setSearching(false);
      return;
    }

    if (!q.trim() || isAiPrefix(q) || isHelpHintQuery(q)) {
      setResults([]);
      setSearching(false);
      return;
    }

    // Plugin Manager shortcut
    if (isPluginManagerQuery(q)) {
      setResults([pluginManagerResult()]);
      setSearching(false);
      return;
    }

    // Slash prefix without a space → show autocomplete suggestions, no backend call
    if (isSlashPrefix(q)) {
      setResults(slashSuggestions(q));
      setSearching(false);
      return;
    }

    // Tag this request — only the latest one is allowed to update results.
    const mySeq = ++searchSeqRef.current;
    setSearching(true);
    try {
      const res = await invoke<QueryResult[]>("search", { query: q });
      if (mySeq !== searchSeqRef.current) return; // stale response — drop
      setResults(res);
      setSearchError(null);
    } catch (e) {
      if (mySeq !== searchSeqRef.current) return;
      setResults([]);
      setSearchError(
        e instanceof Error ? e.message : typeof e === "string" ? e : String(e),
      );
    } finally {
      if (mySeq === searchSeqRef.current) setSearching(false);
    }
  }, []);

  const handleQueryChange = useCallback(
    (value: string) => {
      // Strip the internal "__sel__:" sentinel if it ever surfaces in the
      // visible input (e.g. user backspaces into auto-populated selection).
      if (value.startsWith("__sel__:")) {
        value = value.slice("__sel__:".length);
      }
      setHistoryIdx(-1);
      if (isHelpQuery(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setQuery(value);
        setResults(helpResults());
        return;
      }

      if (isHelpHintQuery(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setQuery(value);
        setResults([]);
        return;
      }

      if (isConversationResetCommand(value)) {
        if (debounceRef.current) clearTimeout(debounceRef.current);
        setQuery(value);
        setResults([]);
        return;
      }

      if (value.trim() === "?") {
        setAiModeEnabled((prev) => !prev);
        setQuery("");
        setResults([]);
        return;
      }

      setQuery(value);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      if (isSlashPrefix(value)) {
        // Show slash suggestions instantly in both launcher and AI mode
        searchSeqRef.current++; // invalidate any in-flight backend search
        setSearching(false);
        setResults(slashSuggestions(value));
      } else if (!aiModeEnabled) {
        // Don't clear results immediately — let the debounced search replace
        // them. Clearing first causes the window to shrink then re-expand on
        // every keystroke (flash/flicker UX issue).
        debounceRef.current = setTimeout(() => {
          doSearch(value);
        }, SEARCH_DEBOUNCE_MS);
      } else {
        // In AI mode, clear slash suggestions when user types past the prefix
        searchSeqRef.current++;
        setSearching(false);
        setResults([]);
      }
    },
    [aiModeEnabled, doSearch, setAiModeEnabled, setHistoryIdx],
  );

  return {
    query,
    setQuery,
    results,
    setResults,
    searching,
    searchError,
    searchSeqRef,
    debounceRef,
    doSearch,
    handleQueryChange,
  };
}
