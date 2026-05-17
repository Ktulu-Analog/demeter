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


import { useState, useEffect, useCallback } from 'react';
import { API_BASE } from '../constants';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Space {
  id: string;
  name: string;
  [key: string]: unknown;
}

export interface Conversation {
  id: string;
  title: string;
  updated_at: string;
  messages: unknown[];
  space_id?: string | null;
  created_at?: string;
  [key: string]: unknown;
}

// ── Hook : liste des espaces ──────────────────────────────────────────────────

export interface UseSpacesResult {
  spaces: Space[];
  loadingSpaces: boolean;
  reloadSpaces: () => void;
}

export function useSpaces(): UseSpacesResult {
  const [spaces, setSpaces]   = useState<Space[]>([]);
  const [loading, setLoading] = useState(true);
  const [tick, setTick]       = useState(0);
  const reload = () => setTick(t => t + 1);

  useEffect(() => {
    setLoading(true);
    fetch(`${API_BASE}/api-proxy/api/spaces`)
      .then(r => r.ok ? r.json() : Promise.reject(r.statusText))
      .then((data: { spaces?: Space[] }) => setSpaces(data.spaces || []))
      .catch((err: unknown) => console.error('Impossible de charger les espaces :', err))
      .finally(() => setLoading(false));
  }, [tick]);

  return { spaces, loadingSpaces: loading, reloadSpaces: reload };
}

// ── Hook : gestion des conversations persistées ───────────────────────────────

export interface UseConversationsResult {
  conversations: Conversation[];
  saveConversation: (conv: Conversation) => Promise<void>;
  deleteConversation: (id: string) => Promise<void>;
  fetchConversations: () => Promise<void>;
}

export function useConversations(): UseConversationsResult {
  const [conversations, setConversations] = useState<Conversation[]>([]);

  const fetchConversations = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/conversations`);
      if (res.ok) {
        const data: { conversations?: Conversation[] } = await res.json();
        setConversations(data.conversations || []);
      }
    } catch { /* ignore */ }
  }, []);

  useEffect(() => { fetchConversations(); }, [fetchConversations]);

  const saveConversation = useCallback(async (conv: Conversation) => {
    try {
      await fetch(`${API_BASE}/api-proxy/api/conversations`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(conv),
      });
      setConversations(prev => {
        const idx = prev.findIndex(c => c.id === conv.id);
        const updated = idx >= 0 ? [...prev] : [conv, ...prev];
        if (idx >= 0) updated[idx] = conv;
        return updated.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
      });
    } catch { /* ignore */ }
  }, []);

  const deleteConversation = useCallback(async (id: string) => {
    try {
      await fetch(`${API_BASE}/api-proxy/api/conversations/${id}`, { method: 'DELETE' });
      setConversations(prev => prev.filter(c => c.id !== id));
    } catch { /* ignore */ }
  }, []);

  return { conversations, saveConversation, deleteConversation, fetchConversations };
}
