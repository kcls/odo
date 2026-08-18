import { Component, OnInit, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import type { SamlSSOConfig } from '@odo/core';

import { AuthService } from '../../core/auth.service';

/** Error messages for SAML authentication failures (mirrors incident-tracker). */
const SAML_ERROR_MESSAGES: Record<string, string> = {
  no_roles: $localize`SSO authentication successful, but your account has not been granted application access. Please contact your administrator.`,
  saml_auth_failed: $localize`SSO authentication failed. Please try again or use username/password login.`,
  saml_no_token: $localize`SSO authentication completed but no session token was received. Please try again.`,
  saml_token_invalid: $localize`SSO authentication token was invalid. Please try again.`,
};

@Component({
  selector: 'app-login',
  imports: [
    FormsModule,
    MatButtonModule,
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressSpinnerModule,
  ],
  templateUrl: './login.html',
  styleUrl: './login.scss',
})
export class Login implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);

  protected username = '';
  protected password = '';

  protected readonly error = signal('');
  protected readonly loading = signal(false);
  protected readonly processingSamlCallback = signal(false);
  protected readonly sessionExpired = signal(false);
  protected readonly ssoConfigs = signal<SamlSSOConfig[]>([]);
  protected readonly ssoLoading = signal(true);
  /** When SSO is available, the local form stays behind a link. */
  protected readonly showLocalLogin = signal(false);

  private redirectTo: string | null = null;

  async ngOnInit(): Promise<void> {
    const params = this.route.snapshot.queryParamMap;
    this.redirectTo = params.get('redirect_to');
    this.sessionExpired.set(params.get('session_expired') === '1');

    // SAML callback errors arrive as ?error=<code>
    const errorParam = params.get('error');
    if (errorParam) {
      this.error.set(
        SAML_ERROR_MESSAGES[errorParam] ??
          $localize`Sign-in failed. Please try again.`,
      );
      this.clearQueryParams();
    } else if (params.get('sso') === '1') {
      // The SAML ACS set the HttpOnly refresh cookie and redirected back;
      // exchange it for an access token.
      this.clearQueryParams();
      this.processingSamlCallback.set(true);
      try {
        await this.auth.completeSamlLogin();
        this.navigateAfterLogin();
        return;
      } catch {
        this.error.set(SAML_ERROR_MESSAGES['saml_token_invalid']!);
        this.processingSamlCallback.set(false);
      }
    } else if (this.auth.isAuthenticated()) {
      this.navigateAfterLogin();
      return;
    }

    // SSO is optional; failures fall back to the local login form.
    try {
      this.ssoConfigs.set(await this.auth.listSSOConfigs());
    } catch (err) {
      console.error('Failed to fetch SSO configs:', err);
    } finally {
      this.ssoLoading.set(false);
    }
  }

  protected canSubmit(): boolean {
    return !this.loading() && this.username.length > 0 && this.password.length > 0;
  }

  protected async submit(): Promise<void> {
    this.error.set('');
    this.loading.set(true);
    try {
      await this.auth.login(this.username, this.password);
      this.navigateAfterLogin();
    } catch {
      this.error.set($localize`Sign-in failed. Check your username and password.`);
      this.loading.set(false);
    }
  }

  protected async ssoLogin(spId: number): Promise<void> {
    this.error.set('');
    this.loading.set(true);
    try {
      await this.auth.startSSOLogin(spId, this.redirectTo);
    } catch (err) {
      console.error('SSO login failed:', err);
      this.error.set(SAML_ERROR_MESSAGES['saml_auth_failed']!);
      this.loading.set(false);
    }
  }

  private navigateAfterLogin(): void {
    this.router.navigateByUrl(this.redirectTo || '/');
  }

  private clearQueryParams(): void {
    this.router.navigate([], {
      relativeTo: this.route,
      queryParams: this.redirectTo ? { redirect_to: this.redirectTo } : {},
      replaceUrl: true,
    });
  }
}
