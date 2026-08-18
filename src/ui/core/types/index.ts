// Shared types for all UI applications

export interface User {
  id: number;
  username: string;
  email: string;
  first_name?: string;
  last_name?: string;
  display_name?: string;
  roles?: RoleAssignment[];
  is_active?: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface RoleAssignment {
  role: string;
  org_unit: number;
}

export interface LoginRequest {
  username: string;
  password: string;
  org_unit?: number;
}

export interface LoginResponse {
  access_token: string;
  refresh_token?: string;
  token_type: string;
  expires_in?: number;
}

export interface ApiError {
  message: string;
  code?: string;
  details?: any;
}