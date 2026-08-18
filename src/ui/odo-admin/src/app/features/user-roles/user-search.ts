import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { authApi } from '@odo/core';

import { ErrorHandlerService } from '../../core/error-handler.service';
import { OdoTable } from '../../shared/odo-table/odo-table';

/**
 * Shape returned by @odo/core's authApi.searchUsers / getUser. The core API
 * is loosely typed (`Promise<any[]>`), so this narrows it for the two user
 * tools that consume it (user-roles here, and users-search which imports it).
 */
export interface UserRow {
  id: number;
  email: string;
  username?: string | null;
  display_name: string;
  status?: string | null;
}

@Component({
  selector: 'app-user-search',
  imports: [
    FormsModule,
    CdkTableModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressSpinnerModule,
    OdoTable,
  ],
  templateUrl: './user-search.html',
  styleUrl: './user-search.scss',
})
export class UserSearch {
  private readonly router = inject(Router);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly columns = ['name', 'username', 'email', 'status'];

  protected keywords = '';
  protected readonly loading = signal(false);
  protected readonly searched = signal(false);
  protected readonly users = signal<UserRow[]>([]);

  protected async search(): Promise<void> {
    const keywords = this.keywords.trim();
    if (!keywords) return;

    this.loading.set(true);
    try {
      const results = (await authApi.searchUsers({
        keywords,
        limit: 50,
      })) as UserRow[];
      this.users.set(results);
      this.searched.set(true);
    } catch (err) {
      this.errors.show(err, 'User search failed');
    } finally {
      this.loading.set(false);
    }
  }

  protected open(user: UserRow): void {
    void this.router.navigate(['/user-roles', user.id]);
  }
}
