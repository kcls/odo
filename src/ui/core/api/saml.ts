/**
 * SAML SSO API
 * Communicates with odo-auth via HTTP/JSON.
 */

import { getApiBaseUrl } from '../utils/api-config';
import { authApi } from './auth';
import { ensureTokenFresh } from '../utils/token-refresh';

export interface SamlIdP {
  id: string;
  name: string;
  display_name: string;
  metadata_url?: string;
  metadata_xml?: string;
  sso_url: string;
  slo_url?: string;
  entity_id: string;
  is_active: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface SamlSSOConfig {
  sp_id: number;
  label: string;
  idp_id: number;
}

export interface SamlInitiateResponse {
  redirect_url: string;
  request_id: string;
}

export class SamlApi {
  async listIdPs(filters?: { is_active?: boolean }): Promise<SamlIdP[]> {
    await ensureTokenFresh();
    const token = authApi.getToken();

    const response = await fetch(`${getApiBaseUrl()}/api/v1/odo/auth/saml/idps`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
      },
      body: JSON.stringify(filters ?? {}),
    });

    if (!response.ok) throw new Error(`Failed to fetch IdP list: ${response.status}`);
    const result = await response.json() as { idps?: SamlIdP[] };
    return result.idps || [];
  }

  async listSSOConfigs(origin: string): Promise<SamlSSOConfig[]> {
    const response = await fetch(`${getApiBaseUrl()}/api/v1/odo/auth/saml/sso-configs`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ origin }),
    });

    if (!response.ok) throw new Error(`Failed to fetch SSO configs: ${response.status}`);
    const result = await response.json() as { sso_configs?: SamlSSOConfig[] };
    return result.sso_configs || [];
  }

  async initiateSSOLogin(
    params: { spId: number; relayState?: string }
  ): Promise<SamlInitiateResponse> {
    const query = new URLSearchParams();
    query.append('sp_id', params.spId.toString());
    if (params.relayState) query.append('relay_state', params.relayState);

    const url = `${getApiBaseUrl()}/api/v1/odo/auth/saml/sso/initiate?${query.toString()}`;
    return { redirect_url: url, request_id: '' };
  }
}

export const samlApi = new SamlApi();
