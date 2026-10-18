export type OutputFormat = 'latex' | 'mdInline' | 'mdBlock' | 'wordLatex';
export type Page = 'result' | 'settings' | 'capture';
export type Phase = 'dormant' | 'selecting' | 'recognizing' | 'ready' | 'copying' | 'error';
export interface AppError { code: string; message: string; retryable: boolean }
export interface Settings {
  schemaVersion: number; baseUrl: string; model: string; defaultFormat: OutputFormat;
  shortcut: string; revision: number; hasApiKey: boolean; warning: string | null;
}
export interface SettingsInput {
  baseUrl: string; model: string; apiKey: string | null;
  shortcut: string; defaultFormat: OutputFormat;
}
export interface Session {
  id: string; generation: number; imageToken: string; width: number; height: number;
  latex: string; format: OutputFormat; elapsedMs: number | null;
}
export interface Capture {
  generation: number; imageToken: string; width: number; height: number;
}
export interface ViewState {
  windowGeneration: number; page: Page; phase: Phase; session: Session | null;
  capture: Capture | null; settings: Settings; error: AppError | null; notice: string | null;
  platform: 'macos' | 'windows';
}
export interface Selection {
  x: number; y: number; width: number; height: number; viewportWidth: number; viewportHeight: number;
}
export interface Recognition { latex: string; elapsedMs: number }
export const formats: { value: OutputFormat; label: string; detail: string }[] = [
  { value: 'latex', label: '纯 LaTeX', detail: '不带定界符' },
  { value: 'mdInline', label: 'Markdown · 行内', detail: '$…$' },
  { value: 'mdBlock', label: 'Markdown · 独立公式', detail: '$$…$$' },
  { value: 'wordLatex', label: 'Word · 公式输入', detail: '粘贴到公式框' },
];
export function shortcutLabel(value: string): string {
  return value.replace(/CommandOrControl/g, /Mac/.test(navigator.platform) ? '⌘' : 'Ctrl')
    .replace(/Command|Super|Meta/g, '⌘').replace(/Control/g, 'Ctrl').replace(/Key(?=[A-Z])/g, '').split('+').join(' + ');
}
export function appError(value: unknown): AppError {
  if (value && typeof value === 'object' && 'message' in value) {
    const e = value as AppError;
    return { code: e.code || 'operation', message: String(e.message), retryable: !!e.retryable };
  }
  if (typeof value === 'string') {
    try { const e = JSON.parse(value); if (e.message) return appError(e); } catch { /* Never show unknown provider payloads. */ }
  }
  return { code: 'operation', message: '操作未完成，请重试。', retryable: true };
}
