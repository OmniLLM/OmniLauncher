/// Debounce window between keystroke and backend `query_all`. 150ms balances
/// "feels instant" against "don't fire a query mid-burst" — every shaved ms
/// here shows up directly as launcher latency.
export const SEARCH_DEBOUNCE_MS = 150;
