// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
//           https://www.gnu.org/licenses/agpl-3.0.html
// Année   : 2026
// ----------------------------------------------------------------------------
// Ce fichier fait partie du projet Demeter.
// Vous pouvez le redistribuer et/ou le modifier selon les termes de la
// licence AGPL-3.0 publiée par la Free Software Foundation.
// ============================================================================


import React, { createContext, useContext, useState, useCallback, useRef, useEffect } from 'react';

// ── Types ─────────────────────────────────────────────────────────────────────

type ToastType = 'success' | 'error' | 'info';

interface Toast {
  id: string;
  msg: string;
  type: ToastType;
  duration: number;
}

type ConfirmType = 'danger' | 'default';

interface ConfirmDialog {
  id: string;
  message: string;
  type?: ConfirmType;
  confirmLabel?: string;
}

interface ConfirmOptions {
  type?: ConfirmType;
  confirmLabel?: string;
}

interface DialogContextValue {
  toast: (msg: string, type?: ToastType, duration?: number) => void;
  confirm: (message: string, options?: ConfirmOptions) => Promise<boolean>;
}

// ── Contexte ───────────────────────────────────────────────────────────────────
const DialogContext = createContext<DialogContextValue | null>(null);

// ── Hook ──────────────────────────────────────────────────────────────────────
export function useDialog(): DialogContextValue {
  const ctx = useContext(DialogContext);
  if (!ctx) throw new Error('useDialog must be used inside <DialogProvider>');
  return ctx;
}

// ── Composant toast  ───────────────────────────────────────────────────────────
function ToastItem({ toast, onRemove }: { toast: Toast; onRemove: (id: string) => void }) {
  useEffect(() => {
    const t = setTimeout(() => onRemove(toast.id), toast.duration || 6000);
    return () => clearTimeout(t);
  }, [toast, onRemove]);

  return (
    <div
      className={`dialog-toast dialog-toast--${toast.type}`}
      role="alert"
      onClick={() => onRemove(toast.id)}
    >
      <span className="dialog-toast__icon">
        {toast.type === 'success' ? '✓' : toast.type === 'error' ? '⚠' : 'ℹ'}
      </span>
      <span className="dialog-toast__msg">{toast.msg}</span>
    </div>
  );
}

// ── Composant confirmation (dialogue) ──────────────────────────────────────────────────
function ConfirmDialogComponent({ dialog, onResolve }: { dialog: ConfirmDialog; onResolve: (id: string, result: boolean) => void }) {
  const handleKey = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') onResolve(dialog.id, false);
    if (e.key === 'Enter') onResolve(dialog.id, true);
  }, [dialog.id, onResolve]);

  useEffect(() => {
    document.addEventListener('keydown', handleKey);
    return () => document.removeEventListener('keydown', handleKey);
  }, [handleKey]);

  return (
    <div
      className="dialog-overlay"
      role="dialog"
      aria-modal="true"
      onClick={e => e.target === e.currentTarget && onResolve(dialog.id, false)}
    >
      <div className="dialog-confirm">
        <div className="dialog-confirm__icon">
          {dialog.type === 'danger' ? '🗑️' : '❓'}
        </div>
        <div className="dialog-confirm__body">
          <p className="dialog-confirm__message">{dialog.message}</p>
        </div>
        <div className="dialog-confirm__actions">
          <button
            className="dialog-confirm__btn dialog-confirm__btn--cancel"
            onClick={() => onResolve(dialog.id, false)}
            autoFocus
          >
            Annuler
          </button>
          <button
            className={`dialog-confirm__btn dialog-confirm__btn--ok${dialog.type === 'danger' ? ' dialog-confirm__btn--danger' : ''}`}
            onClick={() => onResolve(dialog.id, true)}
          >
            {dialog.confirmLabel || 'Confirmer'}
          </button>
        </div>
      </div>
    </div>
  );
}

let _idCounter = 0;
const nextId = () => `dlg_${++_idCounter}`;

export function DialogProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts]     = useState<Toast[]>([]);
  const [confirms, setConfirms] = useState<ConfirmDialog[]>([]);
  const pendingRef = useRef<Record<string, (result: boolean) => void>>({});

  const toast = useCallback((msg: string, type: ToastType = 'success', duration = 6000) => {
    const id = nextId();
    setToasts(prev => [...prev, { id, msg, type, duration }]);
  }, []);

  const removeToast = useCallback((id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  }, []);

  const confirm = useCallback((message: string, options: ConfirmOptions = {}): Promise<boolean> => {
    return new Promise(resolve => {
      const id = nextId();
      pendingRef.current[id] = resolve;
      setConfirms(prev => [...prev, { id, message, ...options }]);
    });
  }, []);

  const resolveConfirm = useCallback((id: string, result: boolean) => {
    setConfirms(prev => prev.filter(d => d.id !== id));
    const resolve = pendingRef.current[id];
    if (resolve) {
      resolve(result);
      delete pendingRef.current[id];
    }
  }, []);

  return (
    <DialogContext.Provider value={{ toast, confirm }}>
      {children}
      <div className="dialog-toast-stack" aria-live="polite">
        {toasts.map(t => (
          <ToastItem key={t.id} toast={t} onRemove={removeToast} />
        ))}
      </div>
      {confirms.length > 0 && (
        <ConfirmDialogComponent
          dialog={confirms[0]}
          onResolve={resolveConfirm}
        />
      )}
    </DialogContext.Provider>
  );
}
