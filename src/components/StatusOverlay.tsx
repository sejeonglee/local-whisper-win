import { useEffect, useState, type FormEvent } from "react";
import type { AppSnapshot } from "../lib/app-state";
import { formatBytes } from "../lib/app-state";

interface StatusOverlayProps {
  state: AppSnapshot;
  onSaveHotkey: (hotkey: string) => Promise<string | null>;
  onSaveAsrEngine: (asrEngine: string) => Promise<string | null>;
}

type DashboardTab = "overview" | "runtime" | "product";
type AsrEngine = "whisper" | "qwen3";

interface PhaseContent {
  badge: string;
  title: string;
  summary: string;
  focus: string;
  tone: "calm" | "active" | "working" | "error";
}

interface RoadmapItem {
  stage: "Now" | "Next" | "Later";
  title: string;
  description: string;
}

interface ToneSample {
  app: string;
  text: string;
}

interface DictionaryEntry {
  label: string;
  source: "auto" | "manual";
}

const phaseContent: Record<AppSnapshot["phase"], PhaseContent> = {
  starting: {
    badge: "Booting",
    title: "WhisperWindows dashboard",
    summary: "The app is preparing the local dictation loop and wiring the sidecar into the desktop shell.",
    focus: "Startup and tray readiness",
    tone: "calm",
  },
  downloading_model: {
    badge: "Caching",
    title: "WhisperWindows dashboard",
    summary: "The first launch is downloading the local model so later runs stay fast and private.",
    focus: "Model download progress",
    tone: "working",
  },
  loading_model: {
    badge: "Loading",
    title: "WhisperWindows dashboard",
    summary: "The model is being loaded into memory so the voice loop can respond without a cold start.",
    focus: "Runtime warmup",
    tone: "working",
  },
  ready: {
    badge: "Ready",
    title: "WhisperWindows dashboard",
    summary: "The app is ready to listen, transcribe locally, paste into the last focused app, and restore the clipboard.",
    focus: "Fast dictation loop",
    tone: "active",
  },
  listening_requested: {
    badge: "Arming",
    title: "WhisperWindows dashboard",
    summary: "The shell has accepted the hotkey and is waiting for the sidecar to confirm microphone capture.",
    focus: "Recorder handshake",
    tone: "active",
  },
  listening: {
    badge: "Listening",
    title: "WhisperWindows dashboard",
    summary: "WhisperWindows is capturing audio and keeping the rest of the workflow ready for a quick stop-and-paste.",
    focus: "Capture in progress",
    tone: "active",
  },
  transcribing: {
    badge: "Transcribing",
    title: "WhisperWindows dashboard",
    summary: "Audio capture is done and the sidecar is turning it into local text for paste-back delivery.",
    focus: "Local transcription",
    tone: "working",
  },
  error: {
    badge: "Blocked",
    title: "WhisperWindows dashboard",
    summary: "Something in the sidecar, clipboard, or microphone path needs attention before the loop can recover.",
    focus: "Recovery and diagnostics",
    tone: "error",
  },
};

const tabs: { id: DashboardTab; label: string; hint: string }[] = [
  { id: "overview", label: "Overview", hint: "Status, workflow, and visible surfaces" },
  { id: "runtime", label: "Runtime", hint: "Model, pipeline, hotkey, and diagnostics" },
  { id: "product", label: "Product", hint: "Roadmap, vocabulary, and app-aware ideas" },
];

const workspaceApps = [
  "Slack",
  "Gmail",
  "Linear",
  "VS Code",
  "Notion",
  "Teams",
  "Docs",
  "WhatsApp",
];

const toneSamples: ToneSample[] = [
  { app: "Slack", text: "Looks good." },
  { app: "Email", text: "Looks good," },
  { app: "Chat", text: "Looks good!" },
];

const roadmap: RoadmapItem[] = [
  {
    stage: "Now",
    title: "Voice-state dashboard",
    description: "Keep startup, listening, transcription, and recovery visible without forcing the user into a terminal.",
  },
  {
    stage: "Now",
    title: "Configurable hotkey",
    description: "Choose a preferred global shortcut in the runtime tab and have it apply immediately without a restart.",
  },
  {
    stage: "Next",
    title: "Local cleanup pass",
    description: "Shape punctuation and remove filler words after transcription so the result reads more intentionally.",
  },
  {
    stage: "Later",
    title: "Personal dictionary",
    description: "Capture names, jargon, and bilingual terms locally so repeated dictation gets more reliable.",
  },
  {
    stage: "Later",
    title: "Selected-text actions",
    description: "Add local rewrite, summarize, and translate actions on top of the same desktop shell.",
  },
];

const hotkeyRollout = [
  {
    title: "Persist the selected shortcut",
    description: "The selected combination is written to the app settings file so startup reuses it automatically.",
  },
  {
    title: "Re-register it at runtime",
    description: "Saving a new value unregisters the old shortcut, registers the new one, and updates the dashboard immediately.",
  },
  {
    title: "Keep a safe fallback",
    description: "If registration fails or the new shortcut is invalid, the app keeps the last known working shortcut active.",
  },
];

const presetHotkeys = [
  "Ctrl+H",
  "Ctrl+Shift+H",
  "Ctrl+Alt+Space",
  "Alt+F10",
];

const engineOptions: { id: AsrEngine; title: string; description: string }[] = [
  {
    id: "whisper",
    title: "Whisper",
    description: "Keeps the existing faster-whisper + CTranslate2 path with large-v3-turbo.",
  },
  {
    id: "qwen3",
    title: "Qwen3-ASR",
    description: "Uses the native Windows transformers backend and auto-falls back to 0.6B on tighter 6GB GPUs.",
  },
];

const visibleWindows = [
  {
    title: "Dashboard window",
    description: "The main desktop overview for current status, runtime facts, and product direction.",
  },
  {
    title: "Tray icon",
    description: "A resident tray entry keeps the app available even when the dashboard is hidden.",
  },
  {
    title: "Runtime states",
    description: "Download, loading, listening, transcribing, and error states stay in the same dashboard instead of separate popups.",
  },
];

const localFirstTraits = [
  "Korean",
  "English",
  "Mixed speech",
  "Clipboard restore",
  "Local model cache",
  "Future local LLM cleanup",
];

function getPipeline(state: AppSnapshot) {
  const progress = state.downloadProgress;

  return [
    {
      title: "Warm the local runtime",
      description: progress
        ? `${formatBytes(progress.receivedBytes)} of ${formatBytes(progress.totalBytes)} cached for ${progress.model ?? state.model ?? "large-v3-turbo"}.`
        : `${formatEngineLabel(state.engine)} using ${state.model ?? "large-v3-turbo"} on ${state.backend ?? "local runtime"}.`,
    },
    {
      title: "Listen from the global hotkey",
      description: `Press ${state.hotkey} once to start capture and again to stop.`,
    },
    {
      title: "Transcribe locally",
      description: "The Python sidecar handles audio capture, selected ASR inference, and ready/error events.",
    },
    {
      title: "Paste and restore",
      description: "Rust pastes into the last focused app and then restores the previous clipboard contents.",
    },
  ];
}

function getActivePipelineIndex(phase: AppSnapshot["phase"]): number {
  switch (phase) {
    case "starting":
    case "downloading_model":
    case "loading_model":
      return 0;
    case "ready":
    case "listening_requested":
    case "listening":
      return 1;
    case "transcribing":
      return 2;
    case "error":
      return -1;
  }
}

function formatUpdatedAt(updatedAt: number): string {
  if (updatedAt <= 0) {
    return "--:--:--";
  }

  return new Date(updatedAt).toLocaleTimeString();
}

function formatProgress(state: AppSnapshot): string {
  const progress = state.downloadProgress;
  if (!progress) {
    return "Model cache is ready or already reused.";
  }

  return `${progress.percent ?? 0}% cached (${formatBytes(progress.receivedBytes)} / ${formatBytes(progress.totalBytes)})`;
}

function formatEngineLabel(engine: AppSnapshot["engine"]): string {
  return engine === "qwen3" ? "Qwen3-ASR" : "Whisper";
}

export function StatusOverlay({ state, onSaveHotkey, onSaveAsrEngine }: StatusOverlayProps) {
  const [activeTab, setActiveTab] = useState<DashboardTab>("overview");
  const [hotkeyDraft, setHotkeyDraft] = useState(state.hotkey);
  const [hotkeyNotice, setHotkeyNotice] = useState<string | null>(null);
  const [hotkeyError, setHotkeyError] = useState<string | null>(null);
  const [isSavingHotkey, setIsSavingHotkey] = useState(false);
  const [engineDraft, setEngineDraft] = useState<AsrEngine>((state.engine as AsrEngine | null) ?? "whisper");
  const [engineNotice, setEngineNotice] = useState<string | null>(null);
  const [engineError, setEngineError] = useState<string | null>(null);
  const [isSavingEngine, setIsSavingEngine] = useState(false);

  useEffect(() => {
    if (
      state.phase === "downloading_model" ||
      state.phase === "loading_model" ||
      state.phase === "error"
    ) {
      setActiveTab("runtime");
    }
  }, [state.phase]);

  useEffect(() => {
    setHotkeyDraft(state.hotkey);
  }, [state.hotkey]);

  useEffect(() => {
    setEngineDraft((state.engine as AsrEngine | null) ?? "whisper");
  }, [state.engine]);

  const meta = phaseContent[state.phase];
  const progress = state.downloadProgress;
  const pipeline = getPipeline(state);
  const activePipelineIndex = getActivePipelineIndex(state.phase);
  const dictionaryEntries: DictionaryEntry[] = [
    { label: "WhisperWindows", source: "manual" },
    { label: state.model ?? "large-v3-turbo", source: "auto" },
    { label: "RTX 4050", source: "manual" },
    { label: formatEngineLabel(state.engine), source: "auto" },
    { label: state.backend ?? "CUDA", source: "auto" },
    { label: "Korean + English", source: "auto" },
    { label: state.hotkey, source: "manual" },
  ];
  const isHotkeyDirty = hotkeyDraft.trim() !== state.hotkey;
  const isEngineDirty = engineDraft !== ((state.engine as AsrEngine | null) ?? "whisper");

  async function handleHotkeySubmit(event?: FormEvent<HTMLFormElement>) {
    event?.preventDefault();
    if (!isHotkeyDirty || isSavingHotkey) {
      return;
    }

    setIsSavingHotkey(true);
    setHotkeyNotice(null);
    setHotkeyError(null);
    const error = await onSaveHotkey(hotkeyDraft);
    setIsSavingHotkey(false);

    if (error) {
      setHotkeyError(error);
      return;
    }

    setHotkeyNotice("Shortcut updated. The new global hotkey is active immediately.");
  }

  function applyHotkeyPreset(preset: string) {
    setHotkeyDraft(preset);
    setHotkeyError(null);
    setHotkeyNotice(null);
  }

  async function handleEngineSubmit(nextEngine?: AsrEngine) {
    const selectedEngine = nextEngine ?? engineDraft;
    if ((!isEngineDirty && selectedEngine === ((state.engine as AsrEngine | null) ?? "whisper")) || isSavingEngine) {
      return;
    }

    setIsSavingEngine(true);
    setEngineNotice(null);
    setEngineError(null);
    const error = await onSaveAsrEngine(selectedEngine);
    setIsSavingEngine(false);

    if (error) {
      setEngineError(error);
      return;
    }

    setEngineNotice("ASR engine updated. The sidecar is restarting with the selected model path.");
  }

  function applyEngineOption(engine: AsrEngine) {
    setEngineDraft(engine);
    setEngineError(null);
    setEngineNotice(null);
  }

  return (
    <main className={`shell shell--${state.phase}`}>
      <section className="dashboard">
        <header className="dashboard__header">
          <div className="eyebrow">Local-first Windows dictation</div>

          <div className="dashboard__headline">
            <div>
              <h1>{meta.title}</h1>
              <p className="dashboard__summary">{meta.summary}</p>
            </div>

            <div className="dashboard__chips">
              <span className={`phase-badge phase-badge--${meta.tone}`}>{meta.badge}</span>
              <span className="hotkey-badge">{state.hotkey}</span>
            </div>
          </div>
        </header>

        <section className="hero-grid">
          <article className="panel panel--hero">
            <div className="panel__eyebrow">Current loop</div>
            <h2>{state.message}</h2>
            <p className="panel__copy">
              Focus right now: {meta.focus}. The dashboard is grouped so the most useful status stays visible without
              needing a long scroll.
            </p>

            <div className="metric-grid">
              <article className="metric-card">
                <span>Engine</span>
                <strong>{formatEngineLabel(state.engine)}</strong>
              </article>
              <article className="metric-card">
                <span>Runtime</span>
                <strong>{state.backend ?? "Preflight"}</strong>
              </article>
              <article className="metric-card">
                <span>Model</span>
                <strong>{state.model ?? "large-v3-turbo"}</strong>
              </article>
              <article className="metric-card">
                <span>Updated</span>
                <strong>{formatUpdatedAt(state.updatedAt)}</strong>
              </article>
            </div>

            {progress ? (
              <div className="progress-card">
                <div className="progress-card__header">
                  <span>First-run local cache</span>
                  <span>{progress.percent ?? 0}%</span>
                </div>
                <div className="progress-bar" aria-hidden="true">
                  <div className="progress-bar__fill" style={{ width: `${progress.percent ?? 0}%` }} />
                </div>
                <p className="progress-card__caption">
                  {formatBytes(progress.receivedBytes)} of {formatBytes(progress.totalBytes)}
                </p>
              </div>
            ) : null}
          </article>

          <article className="panel panel--session">
            <div className="panel__eyebrow">Session</div>
            <dl className="detail-grid">
              <div>
                <dt>Phase</dt>
                <dd>{meta.badge}</dd>
              </div>
              <div>
                <dt>Bootstrap</dt>
                <dd>{state.isStubBootstrap ? "Scaffold mode" : "Live runtime"}</dd>
              </div>
              <div>
                <dt>Engine</dt>
                <dd>{formatEngineLabel(state.engine)}</dd>
              </div>
              <div>
                <dt>Hotkey</dt>
                <dd>{state.hotkey}</dd>
              </div>
              <div>
                <dt>Model</dt>
                <dd>{state.model ?? "large-v3-turbo"}</dd>
              </div>
              <div>
                <dt>Cache</dt>
                <dd>{formatProgress(state)}</dd>
              </div>
            </dl>

            {state.lastError ? <div className="error-banner">{state.lastError}</div> : null}
          </article>
        </section>

        <nav className="tab-bar" aria-label="Dashboard sections">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              className={`tab-button ${activeTab === tab.id ? "tab-button--active" : ""}`}
              type="button"
              onClick={() => setActiveTab(tab.id)}
            >
              <span>{tab.label}</span>
              <small>{tab.hint}</small>
            </button>
          ))}
        </nav>

        <section className="tab-panel">
          {activeTab === "overview" ? (
            <div className="card-grid">
              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Workflow</div>
                    <h3>Voice loop at a glance</h3>
                  </div>
                </div>

                <ol className="pipeline">
                  {pipeline.map((step, index) => {
                    const stateClass =
                      activePipelineIndex === index
                        ? "pipeline__item--active"
                        : activePipelineIndex > index
                          ? "pipeline__item--done"
                          : "";

                    return (
                      <li key={step.title} className={`pipeline__item ${stateClass}`}>
                        <div className="pipeline__index">{index + 1}</div>
                        <div>
                          <h4>{step.title}</h4>
                          <p>{step.description}</p>
                        </div>
                      </li>
                    );
                  })}
                </ol>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Surfaces</div>
                    <h3>What shows up in Windows</h3>
                  </div>
                </div>

                <div className="stack-list">
                  {visibleWindows.map((item) => (
                    <article key={item.title} className="stack-list__item">
                      <h4>{item.title}</h4>
                      <p>{item.description}</p>
                    </article>
                  ))}
                </div>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Targets</div>
                    <h3>Cross-app handoff</h3>
                  </div>
                </div>

                <p className="panel__copy">
                  WhisperWindows is meant to stay lightweight and hand text back to the app you were already using.
                </p>

                <div className="pill-cloud">
                  {workspaceApps.map((app) => (
                    <span key={app} className="pill-cloud__item">
                      {app}
                    </span>
                  ))}
                </div>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Principles</div>
                    <h3>Local-first by design</h3>
                  </div>
                </div>

                <p className="panel__copy">
                  The long-term direction is still the same: fast cross-app writing help, but with local inference,
                  local cache reuse, and explicit clipboard safety.
                </p>

                <div className="pill-cloud">
                  {localFirstTraits.map((item) => (
                    <span key={item} className="pill-cloud__item">
                      {item}
                    </span>
                  ))}
                </div>
              </article>
            </div>
          ) : null}

          {activeTab === "runtime" ? (
            <div className="card-grid">
              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Runtime</div>
                    <h3>Model and shell facts</h3>
                  </div>
                </div>

                <dl className="detail-grid">
                  <div>
                    <dt>Engine</dt>
                    <dd>{formatEngineLabel(state.engine)}</dd>
                  </div>
                  <div>
                    <dt>Backend</dt>
                    <dd>{state.backend ?? "preflight"}</dd>
                  </div>
                  <div>
                    <dt>Model</dt>
                    <dd>{state.model ?? "large-v3-turbo"}</dd>
                  </div>
                  <div>
                    <dt>Bootstrap</dt>
                    <dd>{state.isStubBootstrap ? "Scaffold" : "Live"}</dd>
                  </div>
                  <div>
                    <dt>Hotkey</dt>
                    <dd>{state.hotkey}</dd>
                  </div>
                  <div>
                    <dt>Updated</dt>
                    <dd>{formatUpdatedAt(state.updatedAt)}</dd>
                  </div>
                  <div>
                    <dt>Message</dt>
                    <dd>{state.message}</dd>
                  </div>
                </dl>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Cache</div>
                    <h3>Model download status</h3>
                  </div>
                </div>

                {progress ? (
                  <>
                    <div className="progress-card progress-card--inline">
                      <div className="progress-card__header">
                        <span>{progress.model ?? state.model ?? "large-v3-turbo"}</span>
                        <span>{progress.percent ?? 0}%</span>
                      </div>
                      <div className="progress-bar" aria-hidden="true">
                        <div className="progress-bar__fill" style={{ width: `${progress.percent ?? 0}%` }} />
                      </div>
                      <p className="progress-card__caption">
                        {formatBytes(progress.receivedBytes)} of {formatBytes(progress.totalBytes)}
                      </p>
                    </div>
                    <p className="panel__copy">
                      First-run setup is in progress. Once cached, later launches should skip back to model load and then
                      ready.
                    </p>
                  </>
                ) : (
                  <div className="callout">
                    <strong>Cache looks ready.</strong>
                    <span>The runtime is reusing a previously prepared model directory.</span>
                  </div>
                )}
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Engine</div>
                    <h3>Speech model runtime</h3>
                  </div>
                </div>

                <div className="preset-row" aria-label="Available ASR engines">
                  {engineOptions.map((option) => (
                    <button
                      key={option.id}
                      className={`preset-chip ${engineDraft === option.id ? "preset-chip--active" : ""}`}
                      type="button"
                      onClick={() => applyEngineOption(option.id)}
                    >
                      {option.title}
                    </button>
                  ))}
                </div>

                <p className="panel__copy">
                  {engineOptions.find((option) => option.id === engineDraft)?.description}
                </p>

                <div className="hotkey-form__row">
                  <button
                    className="hotkey-save"
                    type="button"
                    disabled={!isEngineDirty || isSavingEngine}
                    onClick={() => void handleEngineSubmit()}
                  >
                    {isSavingEngine ? "Switching..." : "Switch engine"}
                  </button>
                </div>

                {engineNotice ? <div className="callout callout--success">{engineNotice}</div> : null}
                {engineError ? <div className="callout callout--error">{engineError}</div> : null}

                <div className="stack-list">
                  <article className="stack-list__item">
                    <h4>Current engine</h4>
                    <p>{formatEngineLabel(state.engine)} is active in the desktop sidecar right now.</p>
                  </article>
                  <article className="stack-list__item">
                    <h4>Resolved model</h4>
                    <p>{state.model ?? "large-v3-turbo"} is the actual checkpoint the runtime loaded.</p>
                  </article>
                  <article className="stack-list__item">
                    <h4>6GB safety policy</h4>
                    <p>Qwen3-ASR automatically falls back to 0.6B when the GPU does not have enough VRAM for 1.7B.</p>
                  </article>
                </div>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Hotkey</div>
                    <h3>Global shortcut controls</h3>
                  </div>
                </div>

                <form className="hotkey-form" onSubmit={handleHotkeySubmit}>
                  <label className="hotkey-form__label" htmlFor="hotkey-input">
                    Active shortcut
                  </label>
                  <div className="hotkey-form__row">
                    <input
                      id="hotkey-input"
                      className="hotkey-input"
                      value={hotkeyDraft}
                      onChange={(event) => setHotkeyDraft(event.target.value)}
                      placeholder="Ctrl+Shift+H"
                      autoComplete="off"
                      spellCheck={false}
                    />
                    <button
                      className="hotkey-save"
                      type="submit"
                      disabled={!isHotkeyDirty || isSavingHotkey}
                    >
                      {isSavingHotkey ? "Saving..." : "Apply"}
                    </button>
                  </div>
                  <p className="hotkey-form__hint">
                    Examples: Ctrl+Shift+H, Ctrl+Alt+Space, Alt+F10. Changes apply without restarting.
                  </p>
                </form>

                <div className="preset-row" aria-label="Suggested hotkeys">
                  {presetHotkeys.map((preset) => (
                    <button
                      key={preset}
                      className={`preset-chip ${hotkeyDraft === preset ? "preset-chip--active" : ""}`}
                      type="button"
                      onClick={() => applyHotkeyPreset(preset)}
                    >
                      {preset}
                    </button>
                  ))}
                </div>

                {hotkeyNotice ? <div className="callout callout--success">{hotkeyNotice}</div> : null}
                {hotkeyError ? <div className="callout callout--error">{hotkeyError}</div> : null}

                <div className="stack-list">
                  {hotkeyRollout.map((item) => (
                    <article key={item.title} className="stack-list__item">
                      <h4>{item.title}</h4>
                      <p>{item.description}</p>
                    </article>
                  ))}
                </div>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Diagnostics</div>
                    <h3>What to watch while iterating</h3>
                  </div>
                </div>

                <div className="stack-list">
                  <article className="stack-list__item">
                    <h4>Clipboard and paste</h4>
                    <p>Keep verifying focus restore, paste delivery, and clipboard rollback after every dictation cycle.</p>
                  </article>
                  <article className="stack-list__item">
                    <h4>Audio lifecycle</h4>
                    <p>Make sure the sidecar moves cleanly through listening, transcribing, empty-audio, and error states.</p>
                  </article>
                  <article className="stack-list__item">
                    <h4>Packaging path</h4>
                    <p>The release build now prefers bundled Python runtime resources instead of assuming the source tree.</p>
                  </article>
                </div>
              </article>
            </div>
          ) : null}

          {activeTab === "product" ? (
            <div className="card-grid">
              <article className="panel panel--wide">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Roadmap</div>
                    <h3>Grouped product direction</h3>
                  </div>
                </div>

                <div className="roadmap-list">
                  {roadmap.map((item) => (
                    <article key={item.title} className="roadmap-item">
                      <div className={`stage-pill stage-pill--${item.stage.toLowerCase()}`}>{item.stage}</div>
                      <div>
                        <h4>{item.title}</h4>
                        <p>{item.description}</p>
                      </div>
                    </article>
                  ))}
                </div>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Tone</div>
                    <h3>App-aware writing previews</h3>
                  </div>
                </div>

                <div className="tone-grid">
                  {toneSamples.map((sample) => (
                    <div key={sample.app} className="tone-card">
                      <div className="tone-card__text">{sample.text}</div>
                      <div className="tone-card__label">{sample.app}</div>
                    </div>
                  ))}
                </div>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Vocabulary</div>
                    <h3>Local dictionary seeds</h3>
                  </div>
                </div>

                <div className="dictionary-grid">
                  {dictionaryEntries.map((entry) => (
                    <div key={entry.label} className="dictionary-item">
                      <span className={`dictionary-item__marker dictionary-item__marker--${entry.source}`} />
                      <span>{entry.label}</span>
                    </div>
                  ))}
                </div>
              </article>

              <article className="panel">
                <div className="panel__header">
                  <div>
                    <div className="section-label">Positioning</div>
                    <h3>Why this feels different</h3>
                  </div>
                </div>

                <p className="panel__copy">
                  The point is not just speech-to-text. It is a small Windows shell that keeps the cross-app loop
                  visible while leaving model download, transcription, and future cleanup local.
                </p>
              </article>
            </div>
          ) : null}
        </section>
      </section>
    </main>
  );
}
