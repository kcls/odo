import { authApi } from './auth';
import { getApiHostPort } from '../utils/api-config';

export interface FileUploadResponse {
  id: string;
  filename: string;
  original_name: string;
  path: string;
  relative_path: string;
  size: number;
  mime_type: string;
  uploaded_by: number;
  uploaded_at: string;
  category: string;
  entity_type?: string;
  entity_id?: string;
}

export interface UploadOptions {
  category?: 'photo' | 'document' | 'video';
  // odo-asset only routes 'incident' and 'patron' to storage directories;
  // any other value is rejected with a 400.
  entity_type?: 'incident' | 'patron';
  entity_id?: string;
  onProgress?: (progress: number) => void;
}

class UploadService {
  async uploadFile(
    file: File,
    options: UploadOptions = {}
  ): Promise<FileUploadResponse> {
    const formData = new FormData();
    formData.append('file', file);

    if (options.category) {
      formData.append('category', options.category);
    }

    if (options.entity_type) {
      formData.append('entity_type', options.entity_type);
    }

    if (options.entity_id) {
      formData.append('entity_id', options.entity_id);
    }

    const token = authApi.getAuthToken();
    if (!token) {
      throw new Error('No authentication token found');
    }

    const xhr = new XMLHttpRequest();

    return new Promise((resolve, reject) => {
      xhr.upload.addEventListener('progress', (event) => {
        if (event.lengthComputable && options.onProgress) {
          const progress = Math.round((event.loaded / event.total) * 100);
          options.onProgress(progress);
        }
      });

      xhr.addEventListener('load', () => {
        if (xhr.status === 200) {
          try {
            const response = JSON.parse(xhr.responseText);
            resolve(response);
          } catch (error) {
            reject(new Error('Failed to parse upload response'));
          }
        } else if (xhr.status === 401) {
          authApi.sessionExpired$.next();
          reject(new Error('Authentication failed'));
        } else if (xhr.status === 400) {
          reject(new Error(xhr.responseText || 'Invalid file or request'));
        } else {
          reject(new Error(`Upload failed with status ${xhr.status}`));
        }
      });

      xhr.addEventListener('error', () => {
        reject(new Error('Network error during upload'));
      });

      xhr.addEventListener('abort', () => {
        reject(new Error('Upload cancelled'));
      });

      let url = this.getBasePath() + 'upload';

      xhr.open('POST', url);
      xhr.setRequestHeader('Authorization', `Bearer ${token}`);
      xhr.send(formData);
    });
  }

  getFileUrl(relativePath: string): string {
    const token = authApi.getAuthToken();

    // For authenticated file access, we need to append the token as a query parameter
    // since we can't set headers on img src attributes
    if (token) {
       return this.getBasePath() + `files/${relativePath}?token=${encodeURIComponent(token)}`;
    }

    return this.getBasePath() + `files/${relativePath}`;
  }

    /** Returns the upload service base path, which may vary in dev
     *  environments in particular.
     *
     *  Points at odo-asset's REST surface (`/api/v1/odo/asset/`) — the
     *  service writes the file to disk *and* the `asset.file_upload`
     *  DB row, so the returned `id` can be passed straight to other
     *  services (e.g. `patron/photo/create`) to bind the upload.
     *  The old `/http-handlers/v1/` legacy upload only wrote to disk
     *  and required the consumer to insert the DB row separately.
     */
    getBasePath(): string {
        return `${window.location.protocol}//${getApiHostPort()}/api/v1/odo/asset/`;
    }


  async fetchFile(relativePath: string): Promise<Blob> {
    const token = authApi.getAuthToken();
    if (!token) {
      throw new Error('No authentication token found');
    }

    let url = this.getBasePath() + `files/${relativePath}`;

    const response = await fetch(url, {headers: {'Authorization': `Bearer ${token}`}});

    if (!response.ok) {
      if (response.status === 401) {
        authApi.sessionExpired$.next();
        throw new Error('Authentication failed');
      } else if (response.status === 404) {
        throw new Error('File not found');
      } else {
        throw new Error(`Failed to fetch file: ${response.status}`);
      }
    }

    return response.blob();
  }

  formatFileSize(bytes: number): string {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
  }

  isImageFile(mimeType: string): boolean {
    return mimeType.startsWith('image/');
  }

  isVideoFile(mimeType: string): boolean {
    return mimeType.startsWith('video/');
  }

  isPDFFile(mimeType: string): boolean {
    return mimeType === 'application/pdf';
  }

  getFileIcon(mimeType: string): string {
    if (this.isImageFile(mimeType)) return 'image';
    if (this.isVideoFile(mimeType)) return 'video_file';
    if (this.isPDFFile(mimeType)) return 'picture_as_pdf';
    if (mimeType.includes('word')) return 'description';
    if (mimeType.includes('text')) return 'text_snippet';
    return 'attachment';
  }

  validateFile(file: File, category: 'photo' | 'document' | 'video'): string | null {
    const maxSizes = {
      photo: 10 * 1024 * 1024,     // 10MB
      document: 50 * 1024 * 1024,   // 50MB
      video: 500 * 1024 * 1024,     // 500MB
    };

    const allowedExtensions = {
      photo: ['.jpg', '.jpeg', '.png', '.gif', '.webp', '.heic'],
      document: ['.pdf', '.doc', '.docx', '.txt', '.rtf', '.odt'],
      video: ['.mp4', '.avi', '.mov', '.wmv', '.flv', '.webm'],
    };

    const maxSize = maxSizes[category];
    if (file.size > maxSize) {
      return `File size exceeds ${this.formatFileSize(maxSize)} limit`;
    }

    const ext = '.' + file.name.split('.').pop()?.toLowerCase();
    const allowed = allowedExtensions[category];
    if (!allowed.includes(ext)) {
      return `File type ${ext} is not allowed for ${category}`;
    }

    return null;
  }
}

export const uploadService = new UploadService();
