/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_URL: string
  readonly VITE_SSE_URL: string
  readonly VITE_METRICS_INTERVAL: string
  readonly VITE_CHAT_AUTO_LOAD_MODELS: string
  readonly VITE_ENABLE_DEV_BYPASS: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
