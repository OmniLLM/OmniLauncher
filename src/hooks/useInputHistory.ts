import { useRef, useState } from "react";

export interface UseInputHistoryResult {
  inputHistory: string[];
  setInputHistory: React.Dispatch<React.SetStateAction<string[]>>;
  historyIdx: number;
  setHistoryIdx: React.Dispatch<React.SetStateAction<number>>;
  inputHistoryRef: React.MutableRefObject<string[]>;
}

export function useInputHistory(): UseInputHistoryResult {
  const [inputHistory, setInputHistory] = useState<string[]>([]);
  const [historyIdx, setHistoryIdx] = useState(-1);
  const inputHistoryRef = useRef<string[]>([]);

  return {
    inputHistory,
    setInputHistory,
    historyIdx,
    setHistoryIdx,
    inputHistoryRef,
  };
}
