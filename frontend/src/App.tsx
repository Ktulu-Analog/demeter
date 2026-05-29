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


import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import './App.css';
import { DialogProvider, useDialog } from './DialogContext';
import { useSpaces, useConversations } from './hooks/index';
import type { Conversation, Space } from './hooks/index';

type LoadableConversation = { id: string; title: string; messages: ChatMessage[]; space_id?: string | null };
import { genId } from './utils/text';
import { hasArtifacts, extractArtifacts } from './utils/artifacts';
import { Message } from './components/Message';
import type { ChatMessage, Attachment } from './components/Message';
import { ArtifactsPanel } from './components/ArtifactsPanel';
import { SettingsModal, McpStatusPanel, AboutModal, McpTooltipButton, type McpEntry } from './components/Modals';
import type { Settings } from './components/Modals';
import { PromptsEditorModal } from './components/PromptsEditorModal';
import { IngestionModal } from './components/IngestionModal';
import { CompareModeView } from './components/CompareMode';
import './CompareMode.css';
import { API_BASE, IMAGE_MIME, IMAGE_EXTS, DOC_EXTS, MIME_TO_EXT, HISTORY_ITEMS } from './constants';

const DEFAULT_SETTINGS: Settings = { endpoint: '', bearer: '', model: '', web_search_mcp: '', web_search_mcp_alias: '', mcp_servers: [] };

// Applique immédiatement les préférences de police sur :root
// Appelé au démarrage ET à chaque changement dans les settings
function applyFontPrefs(fontFamily?: string, fontSize?: number) {
  const FONT_STACKS: Record<string, string> = {
    'dm-sans':   "'DM Sans', sans-serif",
    'inter':     "'Inter', sans-serif",
    'system-ui': "system-ui, -apple-system, sans-serif",
    'ubuntu':    "'Ubuntu', 'Cantarell', sans-serif",
  };
  const root = document.documentElement;
  root.style.setProperty('--font-body', FONT_STACKS[fontFamily ?? 'dm-sans'] ?? FONT_STACKS['dm-sans']);
  root.style.setProperty('--font-size-base', `${fontSize ?? 14}px`);
}

export default function App() {
  return (
    <DialogProvider>
      <AppInner />
    </DialogProvider>
  );
}

interface Usage {
  promptTokens: number;
  completionTokens: number;
  kgCO2eq: number | null;
}

interface CurrentUser {
  name?: string;
  email?: string;
}

function AppInner() {
  const { toast: toastFn } = useDialog();
  const { spaces, loadingSpaces, reloadSpaces } = useSpaces();
  const { conversations, saveConversation, deleteConversation } = useConversations();

  const [settings, setSettings] = useState<Settings>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem('demeter_settings') || '{}');
      const s = { ...DEFAULT_SETTINGS, ...saved };
      // Migration : mcp_servers était string[], normalise en {url, alias}[]
      if (Array.isArray(s.mcp_servers)) {
        s.mcp_servers = s.mcp_servers.map((entry: unknown) =>
          typeof entry === 'string' ? { url: entry } : entry
        );
      }
      applyFontPrefs(s.font_family, s.font_size);
      return s;
    } catch { return DEFAULT_SETTINGS; }
  });
  const initCredentials = (s: Settings) => {
    (window as any).__TAURI__?.core?.invoke('init_credentials', {
      payload: { endpoint: s.endpoint, bearer: s.bearer }
    })?.catch(console.error);
  };

  const saveSettings = (s: Settings) => {
    applyFontPrefs(s.font_family, s.font_size);
    setSettings(s);
    localStorage.setItem('demeter_settings', JSON.stringify(s));
    initCredentials(s);
  };

  const [showSettings,      setShowSettings]      = useState(!settings.endpoint);

  // Initialise AppState Rust dès le démarrage via IPC Tauri (pas HTTP)
  useEffect(() => { initCredentials(settings); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  const [showMcpStatus,     setShowMcpStatus]      = useState(false);
  const [showIngestion,     setShowIngestion]      = useState(false);
  const [showPromptsEditor, setShowPromptsEditor]  = useState(false);
  const [showAbout,         setShowAbout]          = useState(false);
  const [showCompare,       setShowCompare]        = useState(false);

  const [spaceAssignments, setSpaceAssignments] = useState<Record<string, number | null>>(() => {
    try { return JSON.parse(localStorage.getItem('demeter_space_assignments') || '{}'); } catch { return {}; }
  });
  const [collectionNames, setCollectionNames] = useState<Record<number, string>>(() => {
    try { return JSON.parse(localStorage.getItem('demeter_collection_names') || '{}'); } catch { return {}; }
  });
  const reloadAssignments = () => {
    try { setSpaceAssignments(JSON.parse(localStorage.getItem('demeter_space_assignments') || '{}')); } catch { setSpaceAssignments({}); }
    try { setCollectionNames(JSON.parse(localStorage.getItem('demeter_collection_names') || '{}')); } catch { setCollectionNames({}); }
  };

  const [messages,      setMessages]      = useState<ChatMessage[]>([]);
  const inputRef = useRef<HTMLTextAreaElement>(null); // non-contrôlé : évite de re-rendre App sur chaque frappe
  const [loading,       setLoading]       = useState(false);
  const [activeSpaceId, setActiveSpaceId] = useState<string | null>(null);
  const [attachments,   setAttachments]   = useState<Attachment[]>([]);
  const [extracting,    setExtracting]    = useState(false);
  const [webSearch,     setWebSearch]     = useState(false);
  const [lastUsage,     setLastUsage]     = useState<Usage | null>(null);
  const [artifactsMsgIndex, setArtifactsMsgIndex] = useState<number | null>(null);

  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [modelMaxCtx,     setModelMaxCtx]     = useState<Record<string, number>>({});
  const [activeModel,     setActiveModel]     = useState('');
  const [modelDropOpen,   setModelDropOpen]   = useState(false);
  const modelDropRef = useRef<HTMLDivElement>(null);

  const [sidebarOpen,   setSidebarOpen]   = useState(true);
  const [convSearch,    setConvSearch]    = useState('');
  const [currentUser,   setCurrentUser]   = useState<CurrentUser | null>(null);
  const [inputDragOver, setInputDragOver] = useState(false);

  const [currentConvId,    setCurrentConvId]    = useState(() => genId());
  const [currentConvTitle, setCurrentConvTitle] = useState<string | null>(null);
  const titleGeneratedRef = useRef(false);

  const abortControllerRef  = useRef<AbortController | null>(null);
  const chatEndRef          = useRef<HTMLDivElement>(null);
  const chatAreaRef         = useRef<HTMLDivElement>(null);
  const [showScrollBtn, setShowScrollBtn] = useState(false);
  const fileInputRef        = useRef<HTMLInputElement>(null);
  const currentConvIdRef    = useRef(currentConvId);
  const currentConvTitleRef = useRef(currentConvTitle);
  const activeSpaceRef      = useRef<Space | null>(null);
  const settingsRef         = useRef(settings);

  useEffect(() => { currentConvIdRef.current    = currentConvId;    }, [currentConvId]);
  useEffect(() => { currentConvTitleRef.current = currentConvTitle; }, [currentConvTitle]);
  useEffect(() => { settingsRef.current         = settings;         }, [settings]);

  const activeSpace = spaces.find(s => s.id === activeSpaceId) || spaces[0] || null;
  useEffect(() => { activeSpaceRef.current = activeSpace; }, [activeSpace]);

  const configured    = !!(settings.endpoint && settings.bearer && settings.model);
  const artifactsOpen = artifactsMsgIndex !== null;
  const quickPrompts  = (activeSpace as { prompts?: { label: string; icon: string; prompt: string }[] } | null)?.prompts || [];

  useEffect(() => {
    if (!loadingSpaces && spaces.length && activeSpaceId === null) setActiveSpaceId(spaces[0].id);
  }, [loadingSpaces, spaces, activeSpaceId]);

  // Intercepte tous les clics sur liens externes et les ouvre via le shell Tauri
  // (la webview Tauri n'ouvre pas les liens target="_blank" nativement)
  // Utilise window.__TAURI__ injecté globalement (withGlobalTauri: true dans tauri.conf.json)
  useEffect(() => {
    const handleLinkClick = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      const anchor = target.closest('a') as HTMLAnchorElement | null;
      if (!anchor) return;
      const href = anchor.getAttribute('href');
      if (!href || href.startsWith('#') || href.startsWith('javascript')) return;
      if (/^https?:\/\//.test(href) || /^mailto:/.test(href)) {
        e.preventDefault();
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const tauri = (window as any).__TAURI__;
        if (tauri?.shell?.open) {
          tauri.shell.open(href).catch(console.error);
        } else if (tauri?.opener?.open) {
          tauri.opener.open(href).catch(console.error);
        }
      }
    };
    document.addEventListener('click', handleLinkClick, true);
    return () => document.removeEventListener('click', handleLinkClick, true);
  }, []);

  useEffect(() => {
    const lastIdx = [...messages].map((m, i) => m.role === 'assistant' ? i : -1).filter(i => i >= 0).pop();
    if (lastIdx === undefined) return;
    const last = messages[lastIdx];
    if (last.streaming) return;
    if (hasArtifacts(typeof last.content === 'string' ? last.content : '')) setArtifactsMsgIndex(lastIdx);
  }, [messages]);

  useEffect(() => {
    if (!modelDropOpen) return;
    const h = (e: MouseEvent) => {
      if (modelDropRef.current && !modelDropRef.current.contains(e.target as Node)) setModelDropOpen(false);
    };
    document.addEventListener('mousedown', h);
    return () => document.removeEventListener('mousedown', h);
  }, [modelDropOpen]);

  useEffect(() => {
    if (!settings.endpoint || !settings.bearer) return;
    fetch(`${API_BASE}/api-proxy/api/models?endpoint=${encodeURIComponent(settings.endpoint)}`)
      .then(r => r.ok ? r.json() : null)
      .then((data: { data?: { id: string; max_context_length?: number }[] } | null) => {
        const ids = data?.data?.map(m => m.id).filter(Boolean) || [];
        setAvailableModels(ids);
        const ctxMap: Record<string, number> = {};
        data?.data?.forEach(m => { if (m.id && m.max_context_length) ctxMap[m.id] = m.max_context_length; });
        setModelMaxCtx(ctxMap);
        if (ids.length && !activeModel) setActiveModel(settings.model || ids[0]);
      }).catch(() => {});
  }, [settings.endpoint, settings.bearer, settings.model]);

  useEffect(() => {
    if (!settings.endpoint || !settings.bearer) { setCurrentUser(null); return; }
    fetch(`${API_BASE}/api-proxy/api/users/me?endpoint=${encodeURIComponent(settings.endpoint)}`)
      .then(r => r.ok ? r.json() : null).then(setCurrentUser).catch(() => setCurrentUser(null));
  }, [settings.endpoint, settings.bearer]);

  useEffect(() => {
    if (!settings.endpoint || !settings.bearer) return;
    fetch(`${API_BASE}/api-proxy/api/ingestion/collections?endpoint=${encodeURIComponent(settings.endpoint)}`)
      .then(r => r.ok ? r.json() : null)
      .then((data: { data?: { id: number; name: string }[] } | null) => {
        if (!data?.data) return;
        const cache: Record<number, string> = {};
        data.data.forEach(c => { cache[c.id] = c.name; });
        localStorage.setItem('demeter_collection_names', JSON.stringify(cache));
        setCollectionNames(cache);
      }).catch(() => {});
  }, [settings.endpoint, settings.bearer]);

  useEffect(() => {
    const handler = (e: CustomEvent<string>) => {
      const id = e.detail;
      if (id === 'new_conv')       newConversation();
      if (id === 'settings')       setShowSettings(true);
      if (id === 'spaces_editor')  setShowPromptsEditor(true);
      if (id === 'rag')            setShowIngestion(true);
      if (id === 'about')          setShowAbout(true);
      if (id === 'toggle_sidebar') setSidebarOpen(v => !v);
      if (id === 'compare')        setShowCompare(true);
    };
    window.addEventListener('tauri-menu', handler as EventListener);
    return () => window.removeEventListener('tauri-menu', handler as EventListener);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (messages.some(m => m.streaming)) chatEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Affiche/masque le bouton "aller en bas"
  useEffect(() => {
    const el = chatAreaRef.current;
    if (!el) return;
    const onScroll = () => {
      const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
      setShowScrollBtn(distanceFromBottom > 120);
    };
    el.addEventListener('scroll', onScroll, { passive: true });
    onScroll();
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  const scrollToBottom = () => chatEndRef.current?.scrollIntoView({ behavior: 'smooth' });

  const prevStreamingRef = useRef(false);
  useEffect(() => {
    const isStreaming = messages.some(m => m.streaming);
    if (prevStreamingRef.current && !isStreaming)
      requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(() => chatEndRef.current?.scrollIntoView({ behavior: 'smooth' }), 50)));
    prevStreamingRef.current = isStreaming;
  }, [messages]);

  useEffect(() => {
    const isStreaming = messages.some(m => m.streaming);
    if (isStreaming || messages.length < 2) return;
    const lastMsg = messages[messages.length - 1];
    if (lastMsg?.role !== 'assistant' || lastMsg?.streaming) return;
    const doSave = async () => {
      const convId = currentConvIdRef.current;
      const space  = activeSpaceRef.current;
      const cfg    = settingsRef.current;
      let convTitle = currentConvTitleRef.current;
      const now = new Date().toISOString();
      if (!titleGeneratedRef.current) {
        titleGeneratedRef.current = true;
        const firstUser      = messages.find(m => m.role === 'user')?.content || '';
        const firstAssistant = messages.find(m => m.role === 'assistant')?.content || '';
        const firstUserStr   = typeof firstUser === 'string' ? firstUser : (firstUser as Array<{ text?: string }>)[0]?.text || '';
        const firstAssistStr = typeof firstAssistant === 'string' ? firstAssistant : '';
        const fallbackTitle  = firstUserStr.trim().split(/\s+/).slice(0, 6).join(' ') || 'Conversation';
        if (cfg.endpoint && cfg.bearer && cfg.model) {
          try {
            const res = await fetch(`${API_BASE}/api-proxy/api/generate-title`, {
              method: 'POST', headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ first_user: firstUserStr.slice(0, 400), first_assistant: firstAssistStr.slice(0, 400), model: cfg.model }),
            });
            convTitle = res.ok ? ((await res.json()).title || fallbackTitle) : fallbackTitle;
          } catch { convTitle = fallbackTitle; }
        } else { convTitle = fallbackTitle; }
        setCurrentConvTitle(convTitle);
      }
      const serialized = messages.map(m => ({
        role: m.role, content: m.content,
        ...(m.displayContent ? { displayContent: m.displayContent } : {}),
        ...(m.attachments    ? { attachments: m.attachments }       : {}),
      }));
      await saveConversation({ id: convId, title: convTitle || 'Conversation', space_id: space?.id || null, messages: serialized, created_at: now, updated_at: now });
    };
    doSave();
  }, [messages]); // eslint-disable-line

  const autoResize = () => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 120) + 'px';
  };

  const imageFileToAttachment = (file: File): Promise<Attachment> => new Promise((resolve, reject) => {
    const ext = MIME_TO_EXT[file.type] || 'png';
    const reader = new FileReader();
    reader.onload  = () => resolve({ filename: file.name || `image-collée.${ext}`, ext, type: 'image', dataUrl: reader.result as string });
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });

  const addImageAttachments = async (files: File[]) => {
    const images = files.filter(f => IMAGE_MIME.includes(f.type));
    if (!images.length) return;
    try {
      const newAtts = await Promise.all(images.map(imageFileToAttachment));
      setAttachments(prev => { const existing = new Set(prev.map(a => a.filename)); return [...prev, ...newAtts.filter(a => !existing.has(a.filename))]; });
    } catch { toastFn("Impossible de charger l'image", 'error'); }
  };

  const handlePaste = useCallback(async (e: React.ClipboardEvent) => {
    const items = Array.from(e.clipboardData?.items || []).filter(it => IMAGE_MIME.includes(it.type));
    if (!items.length) return;
    e.preventDefault();
    await addImageAttachments(items.map(it => it.getAsFile()).filter(Boolean) as File[]);
  }, []);

  const handleInputDragOver = useCallback((e: React.DragEvent) => {
    if (!Array.from(e.dataTransfer.types).includes('Files')) return;
    e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; setInputDragOver(true);
  }, []);
  const handleInputDragLeave = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    if (e.currentTarget.contains(e.relatedTarget as Node)) return; setInputDragOver(false);
  }, []);
  const handleInputDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault(); setInputDragOver(false);
    const files      = Array.from(e.dataTransfer.files);
    const imageFiles = files.filter(f => IMAGE_MIME.includes(f.type));
    const docFiles   = files.filter(f => DOC_EXTS.includes(f.name.toLowerCase().split('.').pop() || ''));
    if (imageFiles.length) await addImageAttachments(imageFiles);
    if (docFiles.length) {
      const dt = new DataTransfer(); docFiles.forEach(f => dt.items.add(f));
      if (fileInputRef.current) { fileInputRef.current.files = dt.files; fileInputRef.current.dispatchEvent(new Event('change', { bubbles: true })); }
    }
  }, []);

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const selectedFiles = Array.from(e.target.files || []);
    if (fileInputRef.current) fileInputRef.current.value = '';
    if (!selectedFiles.length) return;
    const getExt  = (f: File) => f.name.toLowerCase().split('.').pop() || '';
    const invalid = selectedFiles.filter(f => !([...IMAGE_EXTS, ...DOC_EXTS].includes(getExt(f))));
    if (invalid.length) { toastFn(`Fichiers non supportés : ${invalid.map(f => f.name).join(', ')}`, 'error'); return; }
    const images = selectedFiles.filter(f => IMAGE_EXTS.includes(getExt(f)));
    const docs   = selectedFiles.filter(f => DOC_EXTS.includes(getExt(f)));
    setExtracting(true);
    try {
      const imageAttachments = await Promise.all(images.map(f => new Promise<Attachment>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload  = () => resolve({ filename: f.name, ext: getExt(f), type: 'image', dataUrl: reader.result as string });
        reader.onerror = reject;
        reader.readAsDataURL(f);
      })));
      let docAttachments: Attachment[] = [];
      if (docs.length) {
        const formData = new FormData(); docs.forEach(f => formData.append('files', f));
        const res = await fetch(`${API_BASE}/api-proxy/api/extract-multiple`, { method: 'POST', body: formData });
        if (!res.ok) { const err = await res.json().catch(() => ({ detail: res.statusText })); toastFn(`Erreur : ${err.detail || res.statusText}`, 'error'); return; }
        const data: { errors?: { filename: string; error: string }[]; files?: Attachment[] } = await res.json();
        if (data.errors?.length) { const msgs = data.errors.map(e => `• ${e.filename} : ${e.error}`).join('\n'); if (!data.files?.length) { toastFn(`Erreurs :\n${msgs}`, 'error'); return; } toastFn(`Avertissement :\n${msgs}`, 'error'); }
        docAttachments = data.files || [];
      }
      setAttachments(prev => { const existing = new Set(prev.map(a => a.filename)); return [...prev, ...imageAttachments.filter(a => !existing.has(a.filename)), ...docAttachments.filter(a => !existing.has(a.filename))]; });
    } catch (err) { toastFn(`Erreur lors du chargement : ${(err as Error).message}`, 'error'); }
    finally { setExtracting(false); }
  };

  const stopStreaming = useCallback(() => { if (abortControllerRef.current) { abortControllerRef.current.abort(); abortControllerRef.current = null; } }, []);

  const newConversation = () => {
    setMessages([]); if (inputRef.current) inputRef.current.value = ''; setAttachments([]); setArtifactsMsgIndex(null);
    setCurrentConvId(genId()); setCurrentConvTitle(null); titleGeneratedRef.current = false;
    if (inputRef.current) inputRef.current.style.height = 'auto';
  };
  const switchSpace = (spaceId: string) => { setActiveSpaceId(spaceId); newConversation(); };
  const loadConversation = (conv: { id: string; title: string; messages: ChatMessage[]; space_id?: string | null }) => {
    setMessages(conv.messages || []); setCurrentConvId(conv.id); setCurrentConvTitle(conv.title);
    titleGeneratedRef.current = true; setArtifactsMsgIndex(null); if (inputRef.current) inputRef.current.value = ''; setAttachments([]);
    if (conv.space_id) setActiveSpaceId(conv.space_id);
    if (inputRef.current) inputRef.current.style.height = 'auto';
  };

  const _dispatchChat = useCallback(async ({ historyForApi }: { historyForApi: { role: string; content: unknown }[] }) => {
    setLoading(true);
    const controller = new AbortController(); abortControllerRef.current = controller;
    try {
      const assignments   = (() => { try { return JSON.parse(localStorage.getItem('demeter_space_assignments') || '{}'); } catch { return {}; } })();
      const assignedColId = activeSpace?.id ? assignments[activeSpace.id] : null;
      const baseModel     = activeModel || settings.model;
      const lastUserText  = historyForApi.filter(m => m.role === 'user').pop()?.content || '';
      const lastUserStr   = typeof lastUserText === 'string' ? lastUserText : (lastUserText as Array<{ text?: string }>)[0]?.text || '';
      let modelForRequest = baseModel;
      if (/\b(word|docx|\.docx|document word|fichier word|rapport word)\b/i.test(lastUserStr) && /gpt/i.test(baseModel)) {
        const mistral = availableModels.find(m => /mistral/i.test(m));
        if (mistral) { modelForRequest = mistral; toastFn(`📄 Document Word — modèle basculé vers ${mistral}`, 'info'); }
      }
      const res = await fetch(`${API_BASE}/api-proxy/api/chat`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, signal: controller.signal,
        body: JSON.stringify({ messages: historyForApi, model: modelForRequest, stream: true, space_id: activeSpace?.id || null, mcp_servers: [...(settings.mcp_servers || []).map(s => s.url), ...(webSearch && settings.web_search_mcp ? [settings.web_search_mcp] : [])], ...(assignedColId ? { collection_id: assignedColId } : {}) }),
      });
      if (!res.ok) { throw new Error(`Erreur ${res.status} : ${await res.text()}`); }
      const reader = res.body!.getReader(); const decoder = new TextDecoder();
      let tail = ''; let capturedUsage: Usage | null = null; let hasMcpImage = false;
      // Parser SSE orienté blocs (séparateur \n\n) — robuste aux grands payloads
      const dispatchSseBlock = (block: string) => {
        let eventType: string | null = null;
        let dataLines: string[] = [];
        for (const line of block.split('\n')) {
          if (line.startsWith('event:')) { eventType = line.slice(6).trim(); }
          else if (line.startsWith('data:'))  { dataLines.push(line.slice(5).trim()); }
        }
        const raw = dataLines.join(''); // les lignes data: sont concaténées (spec SSE)
        if (!raw || raw === '[DONE]') return raw === '[DONE]' ? 'DONE' : null;
        if (eventType === 'rag_sources') {
          try { const p = JSON.parse(raw); if (p.sources) setMessages(prev => { const m = [...prev]; m[m.length-1] = { ...m[m.length-1], ragSources: p.sources }; return m; }); } catch { /* ignore */ }
          return null;
        }
        if (eventType === 'mcp_image') {
          try {
            console.debug('[mcp_image] raw.length=', raw.length, 'prefix=', raw.slice(0, 40));
            const p = JSON.parse(raw);
            console.debug('[mcp_image] parsed OK b64_len=', p.b64?.length, 'mime=', p.mime);
            if (p.b64 && p.mime) {
              const bt = '```';
              const mdImg = bt + 'screenshot\n' + JSON.stringify({ mime: p.mime, alt: p.alt || 'Screenshot' }) + '\n' + p.b64 + '\n' + bt;
              hasMcpImage = true;
              setMessages(prev => { const m = [...prev]; const last = m[m.length-1]; m[m.length-1] = { ...last, content: (typeof last.content === 'string' ? last.content : '') + '\n\n' + mdImg }; return m; });
            } else { console.warn('[mcp_image] b64/mime manquant', Object.keys(p)); }
          } catch(e) { console.error('[mcp_image] parse error', e, 'raw.length=', raw.length); }
          return null;
        }
        // Event standard (delta LLM)
        try {
          const parsed = JSON.parse(raw); if (parsed.error) throw new Error(JSON.stringify(parsed.error));
          const delta = parsed.choices?.[0]?.delta?.content;
          if (delta && !hasMcpImage) setMessages(prev => { const m = [...prev]; const last = m[m.length-1]; m[m.length-1] = { ...last, content: (typeof last.content === 'string' ? last.content : '') + delta, streaming: true }; return m; });
          if (parsed.usage) capturedUsage = { promptTokens: parsed.usage.prompt_tokens ?? 0, completionTokens: parsed.usage.completion_tokens ?? 0, kgCO2eq: parsed.usage.impacts?.kgCO2eq ?? null };
        } catch { /* ignore */ }
        return null;
      };
      outer: while (true) {
        const { done, value } = await reader.read();
        if (value) tail += decoder.decode(value, { stream: !done });
        if (done) break;
        // Traiter tous les blocs SSE complets (séparés par \n\n)
        let sepIdx: number;
        while ((sepIdx = tail.indexOf('\n\n')) !== -1) {
          const block = tail.slice(0, sepIdx);
          tail = tail.slice(sepIdx + 2);
          if (dispatchSseBlock(block) === 'DONE') break outer;
        }
      }
      if (capturedUsage) setLastUsage(capturedUsage);
    } catch (err) {
      if ((err as Error).name !== 'AbortError') setMessages(prev => { const m = [...prev]; m[m.length-1] = { role: 'assistant', content: `❌ ${(err as Error).message}`, streaming: false }; return m; });
    } finally {
      abortControllerRef.current = null;
      setMessages(prev => { const m = [...prev]; const last = m[m.length-1]; if (last?.role === 'assistant') m[m.length-1] = { ...last, streaming: false }; return m; });
      setLoading(false);
    }
  }, [settings, activeModel, availableModels, activeSpace, webSearch]);

  const sendMessage = useCallback(async (text?: string) => {
    const userText = (text || inputRef.current?.value || '').trim();
    if ((!userText && !attachments.length) || loading) return;
    if (!configured) { setShowSettings(true); return; }
    let fullContent: ChatMessage['content'], displayContent: string;
    let msgAttachments: Attachment[] | null = null;
    if (attachments.length) {
      const images = attachments.filter(a => a.type === 'image');
      const docs   = attachments.filter(a => a.type !== 'image');
      displayContent = userText || `Analyse de : ${attachments.map(a => a.filename).join(', ')}`;
      msgAttachments = attachments.map(a => ({ filename: a.filename, chars: a.chars, ext: a.ext, type: a.type }));
      if (images.length) {
        const parts: Array<{ type: string; text?: string; image_url?: { url: string } }> = [];
        const textPart = [userText || 'Analyse ces images.', ...docs.map(a => `--- Document : ${a.filename} ---\n\n${a.text}`)].join('\n\n');
        if (textPart) parts.push({ type: 'text', text: textPart });
        images.forEach(img => parts.push({ type: 'image_url', image_url: { url: img.dataUrl! } }));
        fullContent = parts;
      } else {
        fullContent = `${userText ? userText + '\n\n' : "Analyse ces documents et résume leur contenu.\n\n"}${docs.map(a => `--- Document : ${a.filename} ---\n\n${a.text}`).join('\n\n')}`;
      }
    } else { fullContent = userText; displayContent = userText; }
    const userMsg: ChatMessage = { role: 'user', content: fullContent, displayContent, attachments: msgAttachments };
    const historyForApi = [...messages, { role: 'user', content: fullContent }];
    setMessages(prev => [...prev, userMsg, { role: 'assistant', content: '', streaming: true }]);
    if (inputRef.current) { inputRef.current.value = ''; autoResize(); } setAttachments([]);
    if (inputRef.current) inputRef.current.style.height = 'auto';
    await _dispatchChat({ historyForApi });
  }, [messages, loading, attachments, configured, _dispatchChat]);

  const regenerateMessage = useCallback(async (assistantMsgIndex: number) => {
    if (loading) return;
    const truncated = messages.slice(0, assistantMsgIndex);
    setMessages([...truncated, { role: 'assistant', content: '', streaming: true }]);
    await _dispatchChat({ historyForApi: truncated.map(m => ({ role: m.role, content: m.content })) });
  }, [messages, loading, _dispatchChat]);

  const editAndResend = useCallback(async (userMsgIndex: number, newText: string) => {
    if (loading || !newText.trim()) return;
    const before = messages.slice(0, userMsgIndex);
    setMessages([...before, { ...messages[userMsgIndex], content: newText, displayContent: newText }, { role: 'assistant', content: '', streaming: true }]);
    await _dispatchChat({ historyForApi: [...before.map(m => ({ role: m.role, content: m.content })), { role: 'user', content: newText }] });
  }, [messages, loading, _dispatchChat]);

  const handleKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
  };

  const activeArtifactsContent = artifactsMsgIndex !== null ? messages[artifactsMsgIndex]?.content : null;
  const activeArtifacts = useMemo(
    () => typeof activeArtifactsContent === 'string' && activeArtifactsContent && artifactsMsgIndex !== null
      ? extractArtifacts(activeArtifactsContent, artifactsMsgIndex)
      : [],
    [activeArtifactsContent, artifactsMsgIndex],
  );
  const closeArtifacts = useCallback(() => setArtifactsMsgIndex(null), []);

  const lastArtifactsMsgIndex = (() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m.role === 'assistant' && !m.streaming && hasArtifacts(typeof m.content === 'string' ? m.content : '')) return i;
    }
    return null;
  })();

  // Injecte les turns d'une session de comparaison dans le chat principal
  // et ferme le mode comparaison.
  const handleSelectResponse = useCallback((
    compareTurns: import('./components/CompareMode').CompareTurn[],
    chosenSlotIndex: 0 | 1,
  ) => {
    const injected: ChatMessage[] = [];
    for (let i = 0; i < compareTurns.length; i++) {
      const t = compareTurns[i];
      injected.push({ role: 'user', content: t.userContent, displayContent: t.displayContent });
      // Pour le dernier turn : slot retenu par l'utilisateur ; pour les précédents : slot A comme contexte.
      const slot = i === compareTurns.length - 1 ? t.slots[chosenSlotIndex] : t.slots[0];
      injected.push({ role: 'assistant', content: slot.content });
    }
    setMessages(prev => [...prev, ...injected]);
    setShowCompare(false);
  }, []);

  return (
    <div className="layout">
      {showSettings      && <SettingsModal settings={settings} onSave={saveSettings} onClose={() => setShowSettings(false)} />}
      {showMcpStatus     && <McpStatusPanel servers={settings.mcp_servers || []} webSearchMcp={settings.web_search_mcp ? { url: settings.web_search_mcp, alias: settings.web_search_mcp_alias } : undefined} webSearchActive={webSearch} onClose={() => setShowMcpStatus(false)} />}
      {showIngestion     && <IngestionModal settings={settings} spaces={spaces} onClose={() => { setShowIngestion(false); reloadAssignments(); }} />}
      {showPromptsEditor && <PromptsEditorModal onClose={() => setShowPromptsEditor(false)} onSpacesUpdated={reloadSpaces} />}
      {showAbout         && <AboutModal onClose={() => setShowAbout(false)} />}
      {showCompare       && (
        <CompareModeView
          settings={settings}
          availableModels={availableModels}
          modelMaxCtx={modelMaxCtx}
          activeSpace={activeSpace}
          webSearch={webSearch}
          onWebSearchChange={setWebSearch}
          collectionId={activeSpace?.id ? (spaceAssignments[activeSpace.id] ?? null) : null}
          attachments={attachments}
          onAttachmentsChange={setAttachments}
          onFilePickerClick={() => fileInputRef.current?.click()}
          onMcpClick={() => setShowMcpStatus(true)}
          extracting={extracting}
          onSelectResponse={handleSelectResponse}
          onClose={() => setShowCompare(false)}
        />
      )}

      <aside className={`sidebar${sidebarOpen ? '' : ' sidebar--closed'}`}>
        <div className="sidebar-logo"><div className="brand-wrap"><div><div className="brand">Demeter</div><div className="tagline">Assistant RH intelligent</div></div></div></div>
        <button className="new-conv-btn" onClick={newConversation}>
          <svg viewBox="0 0 16 16" fill="currentColor" width="13" height="13"><path d="M8 2a1 1 0 011 1v4h4a1 1 0 110 2H9v4a1 1 0 11-2 0V9H3a1 1 0 110-2h4V3a1 1 0 011-1z"/></svg>
          Nouvelle conversation
        </button>
        <div className="sidebar-section">Espaces</div>
        {loadingSpaces
          ? Array.from({ length: 5 }).map((_, i) => <div key={i} className="nav-item nav-item--skeleton" />)
          : spaces.map(space => (
              <div key={space.id} className={`nav-item ${activeSpace?.id === space.id ? 'active' : ''}`} onClick={() => switchSpace(space.id)}>
                <span className="nav-icon">{(space as { icon?: string }).icon}</span>{(space as { label?: string }).label}
                <span style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 4 }}>
                  {spaceAssignments[space.id] != null && <span className="rag-badge" title={String(collectionNames[spaceAssignments[space.id]!] ?? spaceAssignments[space.id])}>RAG</span>}
                  {(space as { dot?: string }).dot && <span className={`dot ${(space as { dot?: string }).dot}`} />}
                </span>
              </div>
            ))}
        {HISTORY_ITEMS.length > 0 && (<><div className="sidebar-section" style={{ marginTop: 12 }}>Historique récent</div>{HISTORY_ITEMS.map((h, i) => (<div key={i} className="nav-item history"><span className="nav-icon">💬</span>{String(h)}</div>))}</>)}
        {conversations.length > 0 && (
          <>
            <div className="sidebar-section" style={{ marginTop: 12 }}>Conversations récentes</div>
            {conversations.length >= 8 && (
              <div className="conv-search-wrap">
                <svg className="conv-search-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" width="13" height="13"><circle cx="6.5" cy="6.5" r="4.5"/><line x1="10.5" y1="10.5" x2="14" y2="14"/></svg>
                <input className="conv-search-input" type="text" placeholder="Rechercher…" value={convSearch} onChange={e => setConvSearch(e.target.value)} spellCheck={false} />
                {convSearch && <button className="conv-search-clear" onClick={() => setConvSearch('')}>✕</button>}
              </div>
            )}
            <div className="conv-list">
              {(conversations as Conversation[]).filter(conv => !convSearch || conv.title.toLowerCase().includes(convSearch.toLowerCase())).map(conv => (
                <div key={conv.id} className={`conv-item ${conv.id === currentConvId ? 'conv-item--active' : ''}`} onClick={() => loadConversation(conv as LoadableConversation)}>
                  <span className="conv-icon">💬</span>
                  <span className="conv-title">{conv.title}</span>
                  <button className="conv-delete" onClick={e => { e.stopPropagation(); if (conv.id === currentConvId) newConversation(); deleteConversation(conv.id); }}>✕</button>
                </div>
              ))}
              {convSearch && (conversations as Conversation[]).filter(c => c.title.toLowerCase().includes(convSearch.toLowerCase())).length === 0 && <div className="conv-no-result">Aucun résultat</div>}
            </div>
          </>
        )}
        <div className="sidebar-spacer" />
      </aside>

      <main className={`main${artifactsOpen ? ' main--with-artifacts' : ''}`}>
        <header className="topbar">
          <div className="topbar-left">
            <button className="sidebar-toggle" onClick={() => setSidebarOpen(o => !o)}>
              <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16"><path fillRule="evenodd" d="M3 5a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 5a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 5a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z" clipRule="evenodd"/></svg>
            </button>
            {messages.length > 0 && (
              <button className="home-btn" onClick={newConversation}>
                <svg viewBox="0 0 20 20" fill="currentColor" width="15" height="15"><path d="M10.707 2.293a1 1 0 00-1.414 0l-7 7a1 1 0 001.414 1.414L4 10.414V17a1 1 0 001 1h4a1 1 0 001-1v-3h2v3a1 1 0 001 1h4a1 1 0 001-1v-6.586l.293.293a1 1 0 001.414-1.414l-7-7z"/></svg>
                Accueil
              </button>
            )}
            <div className="topbar-title">
              {currentConvTitle && <><span className="topbar-conv-title">{currentConvTitle}</span><span className="topbar-sep">·</span></>}
              {activeSpace ? `${(activeSpace as { icon?: string }).icon} ${(activeSpace as { label?: string }).label}` : 'Assistant RH'}
            </div>
          </div>
          <div className="topbar-badges">
            {lastUsage && (
              <div className="topbar-usage" title={`Tokens : ${lastUsage.promptTokens} in / ${lastUsage.completionTokens} out`}>
                <span className="usage-icon">🔢</span>
                <span className="usage-tokens">
                  <span className="usage-in">{lastUsage.promptTokens.toLocaleString()}</span>
                  <span className="usage-sep">→</span>
                  <span className="usage-out">{lastUsage.completionTokens.toLocaleString()}</span>
                </span>
                {lastUsage.kgCO2eq != null && <span className="usage-co2">🌱 {lastUsage.kgCO2eq < 0.001 ? `${(lastUsage.kgCO2eq * 1e6).toFixed(1)} µgCO₂` : lastUsage.kgCO2eq < 1 ? `${(lastUsage.kgCO2eq * 1000).toFixed(2)} gCO₂` : `${lastUsage.kgCO2eq.toFixed(3)} kgCO₂`}</span>}
              </div>
            )}
            {lastArtifactsMsgIndex !== null && (
              <button className={`artifacts-toggle-btn ${artifactsOpen ? 'artifacts-toggle-btn--active' : ''}`} onClick={() => setArtifactsMsgIndex(artifactsOpen ? null : lastArtifactsMsgIndex)}>
                <svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor"><path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 0h5v5H9V9z"/></svg>
                Artéfacts
              </button>
            )}
            {availableModels.length >= 2 && configured && (
              <button
                className={`artifacts-toggle-btn${showCompare ? ' artifacts-toggle-btn--active' : ''}`}
                onClick={() => setShowCompare(v => !v)}
                title="Comparer deux modèles côte à côte"
              >
                <svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor"><path d="M1 2h6v12H1V2zm8 0h6v12H9V2zM2 3v10h4V3H2zm8 0v10h4V3h-4z"/></svg>
                Comparer
              </button>
            )}
            {currentUser && <span className="badge badge-green" style={{ background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary)' }}>👤 {currentUser.name || currentUser.email}</span>}
            {configured ? (
              availableModels.length > 1 ? (
                <div className="model-pill-wrap" ref={modelDropRef}>
                  <button
                    className="model-pill-btn"
                    aria-expanded={modelDropOpen}
                    onClick={() => setModelDropOpen(v => !v)}
                  >
                    <span className="badge-dot" />
                    {activeModel || settings.model}
                    <svg className="model-pill-chevron" viewBox="0 0 16 16" fill="currentColor" width="9" height="9">
                      <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.5" fill="none"/>
                    </svg>
                  </button>
                  {modelDropOpen && (
                    <div className="model-dropdown">
                      {availableModels.map(m => (
                        <button
                          key={m}
                          type="button"
                          className={`model-dropdown-item${m === (activeModel || settings.model) ? ' model-dropdown-item--active' : ''}`}
                          onClick={() => { setActiveModel(m); setModelDropOpen(false); }}
                        >
                          {m === (activeModel || settings.model) ? '✓ ' : '\u00a0\u00a0'}{m}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              ) : (<span className="badge badge-green"><span className="badge-dot" />{activeModel || settings.model}</span>)
            ) : (<span className="badge badge-red" onClick={() => setShowSettings(true)} style={{ cursor: 'pointer' }}>⚠ Non configuré</span>)}
          </div>
        </header>

        <div className="chat-artifacts-wrapper">
          <div className="chat-column">
            <div className="chat-area" ref={chatAreaRef}>
              {messages.length === 0 && (
                <div className="welcome">
                  <div className="welcome-logo"><img src="/ico-demeter.png" alt="Demeter" className="welcome-logo-img" /></div>
                  <h2 className="welcome-title">{activeSpace ? (activeSpace as { label?: string }).label : 'Bonjour\u00a0!'}</h2>
                  <p className="welcome-sub">
                    {activeSpace ? 'Espace dédié — posez vos questions ou choisissez une suggestion ci-dessous.' : 'Votre assistant RH intelligent.'}<br />
                    <span className="welcome-attach-hint">📎 Joignez un PDF, Word ou image — ou collez une image directement (Ctrl+V).</span>
                  </p>
                  <div className="suggestion-grid">
                    {loadingSpaces
                      ? Array.from({ length: 6 }).map((_, i) => <div key={i} className="suggestion-card suggestion-card--skeleton" />)
                      : quickPrompts.map((q: { label: string; icon: string; prompt: string }) => (
                          <button key={q.label} className="suggestion-card" onClick={() => sendMessage(q.prompt)}>
                            <span className="suggestion-icon">{q.icon}</span>
                            <span className="suggestion-label">{q.label}</span>
                            <span className="suggestion-hint">{q.prompt.slice(0, 52)}…</span>
                          </button>
                        ))}
                  </div>
                </div>
              )}
              {messages.map((msg, i) => (
                <Message key={i} msg={msg} msgIndex={i}
                  onOpenArtifacts={setArtifactsMsgIndex}
                  onRegenerate={regenerateMessage}
                  onEditResend={editAndResend}
                  onStop={stopStreaming}
                  isLast={i === messages.length - 1}
                  loading={loading}
                />
              ))}
              <div ref={chatEndRef} style={{ overflowAnchor: 'none', height: 1 }} />
            </div>

            {showScrollBtn && (
              <button className="scroll-to-bottom-btn" onClick={scrollToBottom} title="Aller en bas">
                <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                  <path d="M8 10.5L2.5 5h11L8 10.5z"/>
                </svg>
              </button>
            )}

            <div className="input-zone">
              <input ref={fileInputRef} type="file" accept=".pdf,.docx,.doc,.png,.jpg,.jpeg,.gif,.webp" style={{ display: 'none' }} multiple onChange={handleFileChange} />
              <div className={`input-box ${loading ? 'input-box--loading' : ''} ${inputDragOver ? 'input-box--dragover' : ''}`} onDragOver={handleInputDragOver} onDragLeave={handleInputDragLeave} onDrop={handleInputDrop}>
                {inputDragOver && (
                  <div className="input-drop-overlay">
                    <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" strokeWidth="1.5"><path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"/></svg>
                    Déposer l'image ici
                  </div>
                )}
                {attachments.length > 0 && (
                  <div className="input-attachments">
                    {attachments.map((att, i) => (
                      <div key={i} className={`input-attach-chip ${att.type === 'image' ? 'input-attach-chip--image' : ''}`}>
                        {att.type === 'image' ? (
                          <div className="attach-img-thumb-wrap">
                            <img src={att.dataUrl} alt={att.filename} className="attach-img-thumb" />
                            <div className="attach-img-preview"><img src={att.dataUrl} alt={att.filename} /></div>
                          </div>
                        ) : <span className="attach-icon">{att.ext === 'pdf' ? '📄' : '📝'}</span>}
                        <span className="attach-name">{att.filename}</span>
                        {att.type !== 'image' && att.chars != null && <span className="attach-chars">{(att.chars / 1000).toFixed(1)}k car.</span>}
                        <button className="attach-remove" onClick={() => setAttachments(prev => prev.filter((_, j) => j !== i))}>✕</button>
                      </div>
                    ))}
                  </div>
                )}
                <textarea ref={inputRef} className="chat-input"
                  placeholder={extracting ? 'Extraction en cours…' : attachments.length === 1 ? `Message à propos de "${attachments[0].filename}"…` : attachments.length > 1 ? `Message à propos de ${attachments.length} documents…` : configured ? `Posez votre question — ${(activeSpace as { label?: string })?.label || 'Assistant RH'}…` : "⚙ Configurez l'endpoint d'abord…"}
                  onChange={autoResize} onKeyDown={handleKey} onPaste={handlePaste} disabled={loading || extracting} rows={3}
                />
                <div className="input-toolbar">
                  <button className={`attach-btn ${extracting ? 'attach-btn--loading' : ''} ${attachments.length ? 'attach-btn--active' : ''}`} onClick={() => fileInputRef.current?.click()} disabled={loading || extracting}>
                    {extracting ? <span className="spinner-sm" /> : <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16"><path fillRule="evenodd" d="M8 4a3 3 0 00-3 3v4a5 5 0 0010 0V7a1 1 0 112 0v4a7 7 0 11-14 0V7a5 5 0 0110 0v4a3 3 0 11-6 0V7a1 1 0 012 0v4a1 1 0 102 0V7a3 3 0 00-3-3z" clipRule="evenodd"/></svg>}
                  </button>
                  <McpTooltipButton
                    className={`websearch-pill ${webSearch ? 'websearch-pill--on' : ''}`}
                    onClick={() => { setWebSearch(v => !v); if (!webSearch) setShowMcpStatus(true); }}
                    entries={settings.web_search_mcp ? [{ url: settings.web_search_mcp, alias: settings.web_search_mcp_alias || undefined }] : []}
                    label={<>
                      <svg viewBox="0 0 16 16" fill="currentColor" width="11" height="11"><path d="M8 1a7 7 0 100 14A7 7 0 008 1zM2.05 8.5h2.01c.06.93.22 1.8.46 2.55A5.01 5.01 0 012.05 8.5zm0-1h2.01A9.2 9.2 0 014.52 4.95 5.01 5.01 0 012.05 7.5zm5.45 5.97V11h-1.7c.22.75.52 1.36.85 1.78.27.35.57.55.85.55v.14zm0-3.47H5.07c-.06-.8-.1-1.62-.1-2.5h2.53v2.5zm0-3.5H4.97c0-.88.04-1.7.1-2.5H7.5V6.5zm0-3.47V1.14c-.28 0-.58.2-.85.55-.33.42-.63 1.03-.85 1.78h1.7v.14zM8.5 13.97V11h1.7c-.22.75-.52 1.36-.85 1.78-.27.35-.57.55-.85.55v.14zm0-3.47V8h2.53c0 .88-.04 1.7-.1 2.5H8.5zm0-3.5V4.5h2.43c.06.8.1 1.62.1 2.5H8.5zm0-3.47V1.14c.28 0 .58.2.85.55.33.42.63 1.03.85 1.78H8.5v.14z"/></svg>
                      {webSearch && settings.web_search_mcp_alias ? settings.web_search_mcp_alias : 'Web'}
                    </>}
                  />
                  {(settings.mcp_servers || []).length > 0 && (
                    <McpTooltipButton
                      className="websearch-pill websearch-pill--on mcp-active-pill"
                      onClick={() => setShowMcpStatus(true)}
                      entries={(settings.mcp_servers || []) as McpEntry[]}
                      label={<>
                        🔌 {(settings.mcp_servers || []).length === 1
                          ? ((settings.mcp_servers![0].alias) || `1 MCP`)
                          : `${settings.mcp_servers!.length} MCP`}
                      </>}
                    />
                  )}
                  <span className="input-hint">Shift+Entrée pour sauter une ligne</span>
                  <button className="send-btn" onClick={() => sendMessage()} disabled={loading || extracting || (!(inputRef.current?.value?.trim()) && !attachments.length)}>
                    {loading ? <span className="spinner" /> : <svg viewBox="0 0 16 16"><path d="M2 8l12-6-4 6 4 6z"/></svg>}
                  </button>
                </div>
              </div>
            </div>
          </div>

          {artifactsOpen && activeArtifacts.length > 0 && (
            <ArtifactsPanel artifacts={activeArtifacts} onClose={closeArtifacts} />
          )}
        </div>
      </main>
    </div>
  );
}
