export type OAuthProvider = {
  id: string;
  label: string;
};

export type LocalOAuthToolStatus = {
  id: string;
  label: string;
  providerId: string;
  cliFound: boolean;
  authDetected: boolean;
  source: string;
  detail?: string;
};

export type CodexAuthStatus = {
  detected: boolean;
  source: string;
  lastRefresh?: string;
  tokenFields: string[];
};

export type CodexConnectivityStatus = {
  ok: boolean;
  expected: string;
  response?: string;
  error?: string;
  command: string;
};

export type LocalCodexReuseResult = {
  reused: boolean;
  profileId?: string;
  model?: string;
  message: string;
  error?: string;
};

export type OAuthLoginResult = {
  providerId: string;
  launched: boolean;
  commandHint: string;
  details: string;
};

export type OllamaStatus = {
  endpoint: string;
  reachable: boolean;
  models: string[];
  error?: string;
};

export type OfficialWebStatus = {
  ready: boolean;
  installed: boolean;
  running: boolean;
  started: boolean;
  url: string;
  commandHint: string;
  message: string;
  error?: string;
};

export type OpenOfficialWebResult = {
  opened: boolean;
  url: string;
  detail: string;
};

export type BootstrapStatus = {
  ready: boolean;
  installed: boolean;
  initialized: boolean;
  web: OfficialWebStatus;
  message: string;
  logs: string[];
  error?: string;
};

export type OpenClawBridge = {
  listOAuthProviders: () => Promise<OAuthProvider[]>;
  detectLocalOAuthTools: () => Promise<LocalOAuthToolStatus[]>;
  startOAuthLogin: (providerId: string) => Promise<OAuthLoginResult>;
  checkOllama: () => Promise<OllamaStatus>;
  bootstrapOpenClaw: () => Promise<BootstrapStatus>;
  ensureOfficialWebReady: () => Promise<OfficialWebStatus>;
  openOfficialWebWindow: () => Promise<OpenOfficialWebResult>;
  saveApiKey: (providerId: string, apiKey: string) => Promise<{ ok: boolean }>;
  detectLocalCodexAuth: () => Promise<CodexAuthStatus>;
  reuseLocalCodexAuth: (setDefaultModel?: boolean) => Promise<LocalCodexReuseResult>;
  validateLocalCodexConnectivity: () => Promise<CodexConnectivityStatus>;
};
