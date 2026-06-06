/**
 * Global keyframes injected once at the top of the App tree. Pulled out so the
 * App component body stays focused on logic, not decoration.
 */
export default function GlobalKeyframes() {
  return (
    <style>{`
      @keyframes omni-fade-in {
        from { opacity: 0; transform: translateY(8px); }
        to   { opacity: 1; transform: translateY(0); }
      }
      @keyframes omni-dot-pulse {
        0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
        40%            { opacity: 1;   transform: scale(1); }
      }
      @keyframes omni-blink {
        0%, 100% { opacity: 1; }
        50%       { opacity: 0; }
      }
      @keyframes omni-spin {
        to { transform: rotate(360deg); }
      }
      .omni-bubble-enter {
        animation: omni-fade-in 200ms ease both;
      }
      .omni-cursor::after {
        content: '|';
        animation: omni-blink 1s step-start infinite;
        color: inherit;
        margin-left: 1px;
      }
    `}</style>
  );
}
