import type { AppSnapshot } from "../lib/app-state";
import { formatBytes } from "../lib/app-state";

interface StatusOverlayProps {
  state: AppSnapshot;
}

interface PhaseContent {
  badge: string;
  title: string;
  summary: string;
  dockLabel: string;
  dockHint: string;
  tone: "calm" | "active" | "working" | "error";
}

interface BlueprintItem {
  stage: "Now" | "Next" | "Later";
  title: string;
  description: string;
}

interface DictionaryEntry {
  label: string;
  source: "auto" | "manual";
}

const phaseContent: Record<AppSnapshot["phase"], PhaseContent> = {
  starting: {
    badge: "Booting",
    title: "Building your local voice layer",
    summary:
      "WhisperWindows is shifting from a plain status panel to a Typeless-inspired writing surface built for local GPU inference.",
    dockLabel: "Bootstrapping the local runtime",
    dockHint: "Warm the model once, then keep the voice loop compact.",
    tone: "calm",
  },
  downloading_model: {
    badge: "Caching",
    title: "Preparing the on-device model",
    summary:
      "Typeless sells instant polish. For WhisperWindows, the equivalent is caching the local model so future dictation stays fast and private.",
    dockLabel: "Downloading the local model",
    dockHint: "First run is setup. Later runs should feel immediate.",
    tone: "working",
  },
  loading_model: {
    badge: "Loading",
    title: "Keeping the model warm",
    summary:
      "The goal is the same seamless voice loop, but backed by Whisper and local hardware instead of a cloud round-trip.",
    dockLabel: "Loading Whisper into memory",
    dockHint: "Once the runtime is warm, the UI can stay lightweight.",
    tone: "working",
  },
  ready: {
    badge: "Ready",
    title: "Speak, polish, paste",
    summary:
      "This project should feel like a cross-app writing layer: hit the hotkey, speak naturally, and let local models turn it into intentional writing.",
    dockLabel: "Ready for dictation",
    dockHint: "Press the hotkey once to start and again to stop.",
    tone: "active",
  },
  listening_requested: {
    badge: "Arming",
    title: "Opening the recorder without taking over",
    summary:
      "A big part of the Typeless feel is that the recorder shows up fast and gets out of the way. WhisperWindows should keep that same rhythm.",
    dockLabel: "Microphone handshake in progress",
    dockHint: "Stay in the app you were already using.",
    tone: "active",
  },
  listening: {
    badge: "Listening",
    title: "Capture the sentence, not the keyboard",
    summary:
      "The UI should feel like a tiny writing instrument, not a dashboard. Speak in Korean, English, or mixed phrases and keep moving.",
    dockLabel: "Listening for your next sentence",
    dockHint: "The best surface here is compact, ambient, and cross-app.",
    tone: "active",
  },
  transcribing: {
    badge: "Polishing",
    title: "Turning raw speech into usable writing",
    summary:
      "Raw transcription is only the start. The next layer is local cleanup for filler removal, punctuation, and app-aware tone.",
    dockLabel: "Transcribing and cleaning locally",
    dockHint: "This is where local LLM cleanup will make the biggest difference.",
    tone: "working",
  },
  error: {
    badge: "Blocked",
    title: "The voice loop needs attention",
    summary:
      "Fast, seamless UX depends on reliable audio, sidecar, and paste behavior. Failures should be obvious and recoverable without guesswork.",
    dockLabel: "Recovery needed before dictation",
    dockHint: "Treat error handling as part of the product feel, not an edge case.",
    tone: "error",
  },
};

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

const toneSamples = [
  { app: "Slack", text: "Looks good." },
  { app: "Email", text: "Looks good," },
  { app: "Chat", text: "Looks good!" },
];

const dictionaryEntries: DictionaryEntry[] = [
  { label: "WhisperWindows", source: "manual" },
  { label: "large-v3-turbo", source: "auto" },
  { label: "RTX 4050", source: "manual" },
  { label: "CTranslate2", source: "manual" },
  { label: "Korean + English", source: "auto" },
  { label: "Ctrl+H", source: "manual" },
];

function getBlueprint(): BlueprintItem[] {
  return [
    {
      stage: "Now",
      title: "Live voice-state overlay",
      description:
        "Keep startup, warmup, listening, transcription, and error feedback visible without pulling the user out of the app they are already in.",
    },
    {
      stage: "Next",
      title: "Local cleanup pass",
      description:
        "Add filler removal, repetition cleanup, and punctuation shaping after Whisper output so the result reads like deliberate writing instead of raw dictation.",
    },
    {
      stage: "Next",
      title: "Personal dictionary",
      description:
        "Store names, jargon, and bilingual terms locally so the system learns project language without sending it to a remote service.",
    },
    {
      stage: "Later",
      title: "Selected-text actions",
      description:
        "Use a local LLM to rewrite, shorten, translate, summarize, or answer questions about highlighted text inside the user's current app.",
    },
    {
      stage: "Later",
      title: "App-aware tone",
      description:
        "Steer cleanup differently for chat, email, docs, issue trackers, or coding tools while keeping the recorder surface small and consistent.",
    },
  ];
}

function getPipeline(state: AppSnapshot) {
  const progress = state.downloadProgress;

  return [
    {
      title: "Warm the local runtime",
      description: progress
        ? `${formatBytes(progress.receivedBytes)} of ${formatBytes(progress.totalBytes)} cached for ${progress.model ?? state.model ?? "large-v3-turbo"}.`
        : `${state.model ?? "large-v3-turbo"} on ${state.backend ?? "local runtime"}.`,
    },
    {
      title: "Listen from the global hotkey",
      description: `Press ${state.hotkey} to start capturing and press it again to stop.`,
    },
    {
      title: "Transcribe and refine on-device",
      description:
        "Whisper handles speech-to-text, then a local cleanup layer can remove filler words and shape punctuation.",
    },
    {
      title: "Paste and restore the clipboard",
      description:
        "Deliver text back into the previously focused app while preserving earlier clipboard formats.",
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

export function StatusOverlay({ state }: StatusOverlayProps) {
  const meta = phaseContent[state.phase];
  const progress = state.downloadProgress;
  const blueprint = getBlueprint();
  const pipeline = getPipeline(state);
  const activePipelineIndex = getActivePipelineIndex(state.phase);

  return (
    <main className={`shell shell--${state.phase}`}>
      <section className="experience">
        <section className="hero">
          <div className="hero__copy">
            <div className="eyebrow">Typeless-inspired local-first dictation</div>
            <h1>{meta.title}</h1>
            <p className="hero__summary">{meta.summary}</p>

            <div className="hero__metrics">
              <article className="metric-card">
                <span>Loop</span>
                <strong>Speak {"->"} clean {"->"} paste</strong>
              </article>
              <article className="metric-card">
                <span>Runtime</span>
                <strong>{state.backend ?? "Preflight"}</strong>
              </article>
              <article className="metric-card">
                <span>Model</span>
                <strong>{state.model ?? "large-v3-turbo"}</strong>
              </article>
            </div>

            <div className="app-strip" aria-label="Target apps">
              {workspaceApps.map((app) => (
                <span key={app} className="app-strip__item">
                  {app}
                </span>
              ))}
            </div>
          </div>

          <aside className="status-card">
            <div className="status-card__header">
              <div>
                <div className="section-label">Session</div>
                <h2>{meta.badge}</h2>
              </div>
              <div className={`phase-badge phase-badge--${meta.tone}`}>{state.hotkey}</div>
            </div>

            <p className="status-card__message">{state.message}</p>

            <dl className="status-grid">
              <div>
                <dt>Backend</dt>
                <dd>{state.backend ?? "preflight"}</dd>
              </div>
              <div>
                <dt>Bootstrap</dt>
                <dd>{state.isStubBootstrap ? "Scaffold mode" : "Live runtime"}</dd>
              </div>
              <div>
                <dt>Model</dt>
                <dd>{state.model ?? "large-v3-turbo"}</dd>
              </div>
              <div>
                <dt>Updated</dt>
                <dd>{formatUpdatedAt(state.updatedAt)}</dd>
              </div>
            </dl>

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

            {state.lastError ? <div className="error-banner">{state.lastError}</div> : null}
          </aside>
        </section>

        <section className="dock-zone" aria-label="Dictation controls">
          <div className="recorder-dock">
            <div className="dock-orb dock-orb--muted">X</div>
            <div className="dock-core">
              <span>{meta.dockLabel}</span>
              <div className={`wave wave--${meta.tone}`} aria-hidden="true">
                <span />
                <span />
                <span />
                <span />
                <span />
              </div>
            </div>
            <div className={`dock-orb dock-orb--${meta.tone}`}>OK</div>
          </div>
          <p className="dock-caption">{meta.dockHint}</p>
        </section>

        <section className="board">
          <article className="card card--wide">
            <div className="card__header">
              <div>
                <div className="section-label">Blueprint</div>
                <h3>Features to port from Typeless</h3>
              </div>
              <p className="card__intro">
                The direction is clear: keep the seamless cross-app writing feel, but replace cloud dependence with
                local models, local memory, and explicit clipboard-safe delivery.
              </p>
            </div>

            <div className="feature-list">
              {blueprint.map((item) => (
                <article key={item.title} className="feature-row">
                  <div className={`stage-pill stage-pill--${item.stage.toLowerCase()}`}>{item.stage}</div>
                  <div>
                    <h4>{item.title}</h4>
                    <p>{item.description}</p>
                  </div>
                </article>
              ))}
            </div>
          </article>

          <article className="card">
            <div className="card__header">
              <div>
                <div className="section-label">Context</div>
                <h3>App-aware writing tones</h3>
              </div>
            </div>

            <div className="tone-preview">
              {toneSamples.map((sample) => (
                <div key={sample.app} className="tone-sample">
                  <div className="tone-sample__bubble">{sample.text}</div>
                  <div className="tone-sample__app">{sample.app}</div>
                </div>
              ))}
            </div>

            <div className="app-chip-grid">
              {workspaceApps.map((app) => (
                <span key={app} className="app-chip-grid__item">
                  {app}
                </span>
              ))}
            </div>
          </article>

          <article className="card">
            <div className="card__header">
              <div>
                <div className="section-label">Vocabulary</div>
                <h3>Local personal dictionary</h3>
              </div>
            </div>

            <div className="dictionary-tabs" aria-hidden="true">
              <span className="dictionary-tabs__item dictionary-tabs__item--active">All</span>
              <span className="dictionary-tabs__item">Auto-added</span>
              <span className="dictionary-tabs__item">Manual</span>
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

          <article className="card">
            <div className="card__header">
              <div>
                <div className="section-label">Pipeline</div>
                <h3>How the local version should work</h3>
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

          <article className="card">
            <div className="card__header">
              <div>
                <div className="section-label">Difference</div>
                <h3>Local-first by design</h3>
              </div>
            </div>

            <div className="privacy-panel">
              <p>
                Typeless markets privacy. WhisperWindows can go further by making local inference, local history, and
                local vocabulary the default architecture instead of a cloud-retention promise.
              </p>

              <div className="language-cloud">
                <span>Korean</span>
                <span>English</span>
                <span>Mixed speech</span>
                <span>Offline cleanup</span>
                <span>Clipboard restore</span>
                <span>Local LLM prompts</span>
              </div>
            </div>
          </article>
        </section>
      </section>
    </main>
  );
}
