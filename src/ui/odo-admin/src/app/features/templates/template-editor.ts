import {
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { MatSlideToggleModule } from '@angular/material/slide-toggle';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatTabsModule } from '@angular/material/tabs';
import { firstValueFrom } from 'rxjs';

import { ApiRequestError } from '../../core/api';
import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { DetailPage } from '../../shared/detail-page/detail-page';
import { ServerErrorStateMatcher } from '../../shared/server-error-state-matcher';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import { templatesApi, type PreviewResponse } from './templates-api';

/**
 * Full-page editor for one notification template. The template is the subject,
 * so the page uses the shared drill-down chrome (`<odo-detail-page>`) with the
 * edit form and a live preview as its body. Preview renders the current
 * (possibly unsaved) form state via the odo-notify preview endpoint.
 */
@Component({
  selector: 'app-template-editor',
  imports: [
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressSpinnerModule,
    MatSlideToggleModule,
    MatTabsModule,
    DetailPage,
  ],
  templateUrl: './template-editor.html',
  styleUrl: './template-editor.scss',
})
export class TemplateEditor {
  /**
   * The `:id` route param (absent on the `new` route). Bound as a raw string
   * via withComponentInputBinding — undefined => creating a new template. (We
   * don't use numberAttribute here: it maps the absent default to NaN, which
   * would be indistinguishable from a real id.)
   */
  readonly id = input<string | undefined>(undefined);

  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly snackBar = inject(MatSnackBar);
  private readonly errors = inject(ErrorHandlerService);

  /** null = creating a new template. */
  private templateId: number | null = null;

  protected readonly loading = signal(true);
  protected readonly isNew = signal(true);
  protected readonly canWrite = signal(false);
  protected readonly saving = signal(false);
  protected readonly selectedTab = signal(0);

  // Form state
  protected code = '';
  protected name = '';
  protected description = '';
  protected subjectTemplate = '';
  protected bodyTemplate = '';
  protected bodyTemplateHtml = '';
  protected sampleData = '';
  protected isActive = true;

  /** Snapshot of the last-saved form state, for dirty tracking. */
  private savedState = '';

  protected readonly formError = signal('');
  protected readonly codeError = signal('');
  protected readonly codeMatcher = new ServerErrorStateMatcher(this.codeError);

  // Preview state
  protected readonly preview = signal<PreviewResponse | null>(null);
  protected readonly previewError = signal('');
  protected readonly previewing = signal(false);
  /** Whether the last rendered preview included unsaved edits. */
  protected readonly previewIsDirty = signal(false);

  /** Subject title shown in the detail-page chrome. */
  protected readonly title = computed(() =>
    this.isNew() ? $localize`:Editor heading for a new template:New Template` : this.name,
  );

  constructor() {
    this.auth.hasPerm(PERMS.TEMPLATE_WRITE).then((ok) => {
      this.canWrite.set(ok);
    });

    // (Re)load whenever the route id changes. The editor is reused across the
    // new/:id routes and between :id values, so ngOnInit would only fire once —
    // an effect tracking id() re-initializes on every change, including the
    // initial one.
    effect(() => {
      const idParam = this.id();
      void this.load(idParam === undefined ? undefined : Number(idParam));
    });
  }

  private async load(id: number | undefined): Promise<void> {
    if (id === undefined || Number.isNaN(id)) {
      this.templateId = null;
      this.isNew.set(true);
      this.savedState = this.currentState();
      this.loading.set(false);
      return;
    }

    // Saving a new template navigates to /templates/:id to fix the URL; the
    // form already holds the saved state, so skip the redundant (racy) refetch.
    if (id === this.templateId && !this.isNew()) {
      return;
    }

    this.loading.set(true);
    this.templateId = id;
    this.isNew.set(false);
    try {
      // No single-get endpoint; template counts are tiny.
      const all = await templatesApi.list();
      const found = all.find((t) => t.id === this.templateId);
      if (!found) {
        this.snackBar.open(
          $localize`Template not found.`,
          $localize`:Snackbar dismiss action:Dismiss`,
        );
        void this.router.navigate(['/templates']);
        return;
      }
      this.code = found.code;
      this.name = found.name;
      this.description = found.description ?? '';
      this.subjectTemplate = found.subject_template;
      this.bodyTemplate = found.body_template;
      this.bodyTemplateHtml = found.body_template_html ?? '';
      this.sampleData = found.sample_data
        ? JSON.stringify(found.sample_data, null, 2)
        : '';
      this.isActive = found.is_active;
      this.savedState = this.currentState();
    } catch (err) {
      this.errors.show(err, 'Failed to load template');
    } finally {
      this.loading.set(false);
    }
  }

  private currentState(): string {
    return JSON.stringify([
      this.code,
      this.name,
      this.description,
      this.subjectTemplate,
      this.bodyTemplate,
      this.bodyTemplateHtml,
      this.sampleData,
      this.isActive,
    ]);
  }

  protected dirty(): boolean {
    return this.currentState() !== this.savedState;
  }

  protected onTabChange(index: number): void {
    this.selectedTab.set(index);
    if (index === 1) {
      void this.refreshPreview();
    }
  }

  protected canSave(): boolean {
    return (
      this.canWrite() &&
      !this.saving() &&
      !!this.code.trim() &&
      !!this.name.trim() &&
      !!this.subjectTemplate.trim() &&
      !!this.bodyTemplate.trim()
    );
  }

  /** Parse the sample-data textarea; returns ok=false on bad JSON. */
  private parseSampleData(): { ok: boolean; value?: unknown } {
    const text = this.sampleData.trim();
    if (!text) return { ok: true, value: undefined };
    try {
      return { ok: true, value: JSON.parse(text) };
    } catch {
      return { ok: false };
    }
  }

  protected async save(): Promise<void> {
    this.formError.set('');
    this.codeError.set('');

    const sample = this.parseSampleData();
    if (!sample.ok) {
      this.formError.set($localize`Sample data must be valid JSON.`);
      return;
    }

    this.saving.set(true);
    const params = {
      code: this.code.trim(),
      name: this.name.trim(),
      description: this.description.trim(),
      subject_template: this.subjectTemplate,
      body_template: this.bodyTemplate,
      body_template_html: this.bodyTemplateHtml,
      is_active: this.isActive,
      ...(sample.value !== undefined ? { sample_data: sample.value } : {}),
    };

    try {
      if (this.templateId === null) {
        const created = await templatesApi.create(params);
        this.templateId = created.id;
        this.isNew.set(false);
        await this.router.navigate(['/templates', created.id], {
          replaceUrl: true,
        });
      } else {
        await templatesApi.update(this.templateId, params);
      }
      this.savedState = this.currentState();
      this.previewIsDirty.set(false);
      this.snackBar.open(
        $localize`Template saved.`,
        $localize`:Snackbar dismiss action:Dismiss`,
        { duration: 3000 },
      );
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'TEMPLATE_CODE_TAKEN') {
        this.codeError.set($localize`A template with this code already exists.`);
      } else {
        this.formError.set(
          err instanceof Error ? err.message : $localize`Failed to save the template.`,
        );
      }
    } finally {
      this.saving.set(false);
    }
  }

  protected async deleteTemplate(): Promise<void> {
    if (this.templateId === null) return;

    const data: ConfirmDialogData = {
      title: $localize`Delete template?`,
      message: $localize`"${this.name}" will be removed. Past notifications that used it are kept.`,
      confirmLabel: $localize`Delete`,
    };
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    const confirmed = await firstValueFrom(ref.afterClosed());
    if (confirmed !== true) return;

    try {
      await templatesApi.delete(this.templateId);
      void this.router.navigate(['/templates']);
    } catch (err) {
      this.errors.show(err, 'Failed to delete template');
    }
  }

  protected async refreshPreview(): Promise<void> {
    this.previewError.set('');

    const sample = this.parseSampleData();
    if (!sample.ok) {
      this.preview.set(null);
      this.previewError.set($localize`Sample data must be valid JSON.`);
      return;
    }

    this.previewing.set(true);
    this.previewIsDirty.set(this.dirty());
    try {
      this.preview.set(
        await templatesApi.preview({
          subject_template: this.subjectTemplate || undefined,
          body_template: this.bodyTemplate || undefined,
          body_template_html: this.bodyTemplateHtml || undefined,
          variables: sample.value,
        }),
      );
    } catch (err) {
      this.preview.set(null);
      this.previewError.set(
        err instanceof Error ? err.message : $localize`Preview failed.`,
      );
    } finally {
      this.previewing.set(false);
    }
  }
}
