/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_MES_EDGE_URL?: string;
  readonly VITE_MES_CLOUD_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
