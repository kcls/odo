import { Component, inject, signal } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatMenuModule } from '@angular/material/menu';
import { MatSidenavModule } from '@angular/material/sidenav';
import { MatToolbarModule } from '@angular/material/toolbar';

import { type AdminTool, visibleAdminTools } from '../core/admin-tools';
import { AuthService } from '../core/auth.service';
import { ThemeService, type ThemeMode } from '../core/theme.service';

/** Authenticated layout: toolbar, nav sidenav, routed content. */
@Component({
  selector: 'app-shell',
  imports: [
    RouterLink,
    RouterLinkActive,
    RouterOutlet,
    MatButtonModule,
    MatIconModule,
    MatMenuModule,
    MatSidenavModule,
    MatToolbarModule,
  ],
  templateUrl: './shell.html',
  styleUrl: './shell.scss',
})
export class Shell {
  protected readonly auth = inject(AuthService);
  protected readonly theme = inject(ThemeService);

  /** Admin tools the user may see; entries appear as perms resolve. */
  protected readonly tools = signal<AdminTool[]>([]);

  constructor() {
    visibleAdminTools(this.auth).then((tools) => this.tools.set(tools));
  }

  protected setTheme(mode: ThemeMode): void {
    this.theme.setMode(mode);
  }

  protected themeIcon(): string {
    switch (this.theme.mode()) {
      case 'light':
        return 'light_mode';
      case 'dark':
        return 'dark_mode';
      default:
        return 'brightness_auto';
    }
  }

  protected displayName(): string {
    const user = this.auth.user();
    return user?.display_name || user?.username || user?.email || '';
  }

  protected async logout(): Promise<void> {
    await this.auth.logout();
  }
}
