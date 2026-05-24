// Shim for browser dev mode - must be imported before any Tauri API imports
// This provides the global Tauri API that @tauri-apps/api expects
if (!(window as any).__TAURI__) {
  const invokeImpl = async (cmd: string, args?: Record<string, unknown>) => {
    throw new Error(
      `Tauri not available in browser. Run the app via 'npx tauri dev' instead.`,
    );
  };

  (window as any).__TAURI__ = {
    core: {
      invoke: invokeImpl,
      convertFileSrc: (path: string) => path,
    },
    event: {
      TauriEvent: {},
      listen: async () => () => {},
      emit: async () => {},
      once: async () => () => {},
    },
    window: {
      WebviewWindow: function () {},
      getCurrent: () => ({
        listen: async () => () => {},
        emit: async () => {},
      }),
    },
    path: {
      join: (...parts: string[]) => parts.join("/"),
      resolve: (...parts: string[]) => parts.join("/"),
    },
    fs: {},
    shell: {},
    dialog: {},
    http: {},
    store: {},
  };
}
