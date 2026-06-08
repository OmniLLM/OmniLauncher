/// <reference types="vite/client" />

declare module "*.rs?raw" {
  const source: string;
  export default source;
}
