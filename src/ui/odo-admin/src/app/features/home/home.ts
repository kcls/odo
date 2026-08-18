import { Component, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';

import { type AdminTool, visibleAdminTools } from '../../core/admin-tools';
import { AuthService } from '../../core/auth.service';

@Component({
  selector: 'app-home',
  imports: [RouterLink, MatCardModule, MatIconModule],
  templateUrl: './home.html',
  styleUrl: './home.scss',
})
export class Home {
  private readonly auth = inject(AuthService);

  protected readonly tools = signal<AdminTool[]>([]);

  constructor() {
    visibleAdminTools(this.auth).then((tools) => this.tools.set(tools));
  }
}
