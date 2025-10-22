/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_URL: string
  readonly VITE_SSE_URL: string
  readonly VITE_METRICS_INTERVAL: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
