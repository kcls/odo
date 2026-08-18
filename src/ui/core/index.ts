// Types
export type {
  User,
  RoleAssignment,
  LoginRequest,
  LoginResponse as AuthLoginResponse,
  ApiError
} from './types';

// API Clients
export { authApi } from './api/auth';
export type {
  TokenClaims,
  LocalLoginRequest,
  LoginResponse,
  ValidateTokenResponse,
  SessionInfo
} from './api/auth';

export { orgUnitApi, OrgUnitApi } from './api/org-units';
export type {
  OrgUnit,
  OrgUnitType,
  OrgUnitTreeResponse,
  OrgUnitDetail,
  OrgUnitOperatingHours
} from './api/org-units';

export { uploadService } from './api/upload';
export type {
  FileUploadResponse,
  UploadOptions
} from './api/upload';

export { samlApi, SamlApi } from './api/saml';
export type {
  SamlIdP,
  SamlSSOConfig,
  SamlInitiateResponse
} from './api/saml';

// API configuration utilities
export { configureApiHostPort, getApiHostPort, getApiBaseUrl } from './utils/api-config';

export {
  configureTokenRefresh,
  ensureTokenFresh,
  isAccessTokenRefreshNeeded,
  getAccessTokenTimeRemaining,
} from './utils/token-refresh';
export type { TokenRefreshConfig } from './utils/token-refresh';
